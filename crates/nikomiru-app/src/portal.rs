// SPDX-License-Identifier: GPL-3.0-only

//! File dialogs through the XDG desktop portal
//! (`org.freedesktop.portal.FileChooser`): the desktop's own dialog, which
//! works in a Flatpak sandbox too (D-5). Web links open in the system's
//! browser.

use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use nikomiru_ui::session::url_to_path;
use nikomiru_ui::{FileDialog, Inbound, Waker, is_web_address};

use crate::dbus::{Connection, Kind, Message, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
const REQUEST: &str = "org.freedesktop.portal.Request";

/// One dialog at a time.
static OPEN: AtomicBool = AtomicBool::new(false);
/// Makes each request's token unique in the process.
static TOKENS: AtomicU32 = AtomicU32::new(1);

/// Opens `dialog` on a thread; the chosen file comes back to the interface as
/// `Inbound::Chosen`, a portal that cannot answer as `Inbound::DialogFailed`.
pub fn open_file(dialog: FileDialog, waker: Arc<dyn Waker>) {
    if OPEN.swap(true, Ordering::SeqCst) {
        tracing::debug!("a file dialog is open already");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("nikomiru-file-dialog".into())
        .spawn(move || {
            let result = Connection::session().and_then(|mut conn| choose(&mut conn, &dialog));
            OPEN.store(false, Ordering::SeqCst);
            match result {
                Ok(Some(path)) => waker.wake(Inbound::Chosen {
                    dialog: dialog.dialog,
                    path,
                }),
                Ok(None) => tracing::debug!("file dialog cancelled"),
                Err(e) => {
                    tracing::warn!("file dialog through the portal: {e}");
                    waker.wake(Inbound::DialogFailed);
                }
            }
        });
    if let Err(e) = spawned {
        OPEN.store(false, Ordering::SeqCst);
        tracing::warn!("file dialog thread: {e}");
    }
}

/// Asks the portal on `conn` for one file and waits for the user; `None` when
/// the dialog is cancelled.
pub fn choose(conn: &mut Connection, dialog: &FileDialog) -> io::Result<Option<PathBuf>> {
    let token = format!("nikomiru{}", TOKENS.fetch_add(1, Ordering::Relaxed));
    // the request's path is known before the call: listen first, so the
    // answer cannot come before (xdg-desktop-portal's Request documentation)
    let sender = conn.unique_name().trim_start_matches(':').replace('.', "_");
    let expected = format!("{PORTAL_PATH}/request/{sender}/{token}");
    listen(conn, &expected)?;
    let reply = conn.call(
        Message::call(PORTAL, PORTAL_PATH, FILE_CHOOSER, "OpenFile").with_body(vec![
            Value::Str(String::new()),
            Value::Str(dialog.title.clone()),
            options(dialog, &token),
        ]),
    )?;
    let Some(Value::Path(handle)) = reply.body.first() else {
        return Err(io::Error::other("OpenFile answered without a request"));
    };
    if *handle != expected {
        listen(conn, handle)?;
    }
    conn.set_timeout(None)?;
    loop {
        let msg = conn.receive()?;
        if msg.kind == Kind::Signal
            && msg.member.as_deref() == Some("Response")
            && msg.path.as_deref() == Some(handle.as_str())
        {
            return Ok(chosen(&msg.body));
        }
    }
}

/// Subscribes to the `Response` of the request at `path`.
fn listen(conn: &mut Connection, path: &str) -> io::Result<()> {
    let rule = format!(
        "type='signal',sender='{PORTAL}',interface='{REQUEST}',member='Response',path='{path}'"
    );
    conn.call(
        Message::call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
        )
        .with_body(vec![Value::Str(rule)]),
    )
    .map(drop)
}

/// The options of `OpenFile`: one file, the dialog's filters (glob patterns),
/// the first one chosen.
pub fn options(dialog: &FileDialog, token: &str) -> Value {
    let filters: Vec<Value> = dialog
        .filters
        .iter()
        .map(|(name, patterns)| {
            Value::Struct(vec![
                Value::Str(name.clone()),
                Value::Array(
                    "(us)".into(),
                    patterns
                        .iter()
                        .map(|p| Value::Struct(vec![Value::U32(0), Value::Str((*p).to_string())]))
                        .collect(),
                ),
            ])
        })
        .collect();
    let mut entries = vec![
        ("handle_token", Value::Str(token.into())),
        ("modal", Value::Bool(true)),
        ("multiple", Value::Bool(false)),
    ];
    if let Some(first) = filters.first() {
        entries.push(("current_filter", first.clone()));
    }
    entries.push(("filters", Value::Array("(sa(us))".into(), filters)));
    Value::dict(&entries)
}

/// The file of a `Response` (`u` response, `a{sv}` results): the first
/// `file://` URI of `uris`, when the user chose one (response 0).
pub fn chosen(body: &[Value]) -> Option<PathBuf> {
    let [Value::U32(0), Value::Array(_, results)] = body else {
        return None;
    };
    let uris = results.iter().find_map(|e| match e {
        Value::Entry(k, v) if **k == Value::Str("uris".into()) => match v.as_ref() {
            Value::Variant(inner) => match inner.as_ref() {
                Value::Array(_, uris) => Some(uris),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })?;
    uris.iter().find_map(|u| match u {
        Value::Str(u) if u.starts_with("file://") => Some(url_to_path(u)),
        _ => None,
    })
}

/// Opens a web address from a dictionary in the browser (POP-011), through
/// `xdg-open` with the address as its only argument. Anything but `http(s)`
/// is refused again here.
pub fn open_uri(url: &str) {
    if !is_web_address(url) {
        tracing::warn!("not a web address, not opened");
        return;
    }
    let url = url.to_string();
    let spawned = std::thread::Builder::new()
        .name("nikomiru-open-uri".into())
        .spawn(move || {
            let status = Command::new("xdg-open")
                .arg(&url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Err(e) = status {
                tracing::warn!("xdg-open: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("{e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_ui::Dialog;

    fn dialog() -> FileDialog {
        FileDialog {
            dialog: Dialog::Subtitles,
            title: "Open subtitles".into(),
            filters: vec![
                ("Subtitles".into(), vec!["*.srt", "*.ass"]),
                ("All files".into(), vec!["*"]),
            ],
        }
    }

    #[test]
    fn the_options_carry_the_token_and_the_filters() {
        let o = options(&dialog(), "nikomiru7");
        assert_eq!(o.signature(), "a{sv}");
        let Value::Array(_, entries) = &o else {
            panic!()
        };
        let get = |key: &str| {
            entries.iter().find_map(|e| match e {
                Value::Entry(k, v) if **k == Value::Str(key.into()) => Some(v.as_ref().clone()),
                _ => None,
            })
        };
        assert_eq!(
            get("handle_token"),
            Some(Value::Variant(Box::new(Value::Str("nikomiru7".into()))))
        );
        let Some(Value::Variant(filters)) = get("filters") else {
            panic!()
        };
        assert_eq!(filters.signature(), "a(sa(us))");
        let Some(Value::Variant(current)) = get("current_filter") else {
            panic!()
        };
        assert_eq!(
            *current,
            Value::Struct(vec![
                Value::Str("Subtitles".into()),
                Value::Array(
                    "(us)".into(),
                    vec![
                        Value::Struct(vec![Value::U32(0), Value::Str("*.srt".into())]),
                        Value::Struct(vec![Value::U32(0), Value::Str("*.ass".into())]),
                    ]
                ),
            ])
        );
    }

    #[test]
    fn the_answer_gives_the_file_or_nothing() {
        let results = |uris: &[&str]| {
            Value::dict(&[(
                "uris",
                Value::Array(
                    "s".into(),
                    uris.iter().map(|u| Value::Str((*u).into())).collect(),
                ),
            )])
        };
        assert_eq!(
            chosen(&[Value::U32(0), results(&["file:///v/ep%2001.srt"])]),
            Some(PathBuf::from("/v/ep 01.srt"))
        );
        assert_eq!(
            chosen(&[Value::U32(1), results(&["file:///v/a.srt"])]),
            None,
            "cancelled"
        );
        assert_eq!(
            chosen(&[Value::U32(0), results(&["https://x/a.srt"])]),
            None
        );
        assert_eq!(chosen(&[Value::U32(0), results(&[])]), None);
    }
}
