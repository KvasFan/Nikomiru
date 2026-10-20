// SPDX-License-Identifier: GPL-3.0-only

//! L4: the D-Bus client and the screensaver inhibition against a private
//! `dbus-daemon`, with a spy in place of the desktop portal.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use std::path::PathBuf;

use nikomiru_app::dbus::{Connection, Kind, Message, Value};
use nikomiru_app::inhibit::PortalInhibitor;
use nikomiru_ui::session::IdleInhibitor;
use nikomiru_ui::{Dialog, FileDialog};

/// A private bus without service activation, so no real portal can start.
struct Bus {
    daemon: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl Bus {
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bus");
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                socket.display()
            ),
        )
        .unwrap();
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is needed by the L4 tests");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            daemon,
            address: address.trim().to_string(),
            _dir: dir,
        }
    }

    fn connect(&self) -> Connection {
        Connection::open(&self.address).unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// What the spy portal saw.
#[derive(Debug, PartialEq)]
enum Seen {
    Inhibit {
        window: String,
        flags: u32,
        reason: String,
    },
    Close {
        path: String,
    },
    OpenFile {
        title: String,
        token: String,
    },
}

/// The string in a variant of an `a{sv}` option, by key.
fn option(options: &[Value], key: &str) -> String {
    options
        .iter()
        .find_map(|e| match e {
            Value::Entry(k, v) if **k == Value::Str(key.into()) => match v.as_ref() {
                Value::Variant(s) => match s.as_ref() {
                    Value::Str(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default()
}

/// Owns `org.freedesktop.portal.Desktop` and answers like the real portal.
fn spy_portal(mut conn: Connection) -> mpsc::Receiver<Seen> {
    let reply = conn
        .call(
            Message::call(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "RequestName",
            )
            .with_body(vec![
                Value::Str("org.freedesktop.portal.Desktop".into()),
                Value::U32(4),
            ]),
        )
        .unwrap();
    assert_eq!(reply.body, [Value::U32(1)], "the spy owns the name");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut requests = 0;
        while let Ok(msg) = conn.receive() {
            if msg.kind != Kind::MethodCall {
                continue;
            }
            let answer = match msg.member.as_deref() {
                // the user picks a file, or cancels when the title says so
                Some("OpenFile") => {
                    let [Value::Str(_), Value::Str(title), Value::Array(_, options)] =
                        &msg.body[..]
                    else {
                        panic!("OpenFile with body {:?}", msg.body);
                    };
                    let token = option(options, "handle_token");
                    let sender = msg
                        .sender
                        .clone()
                        .unwrap_or_default()
                        .trim_start_matches(':')
                        .replace('.', "_");
                    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
                    let _ = tx.send(Seen::OpenFile {
                        title: title.clone(),
                        token,
                    });
                    let reply = Message::reply_to(&msg, vec![Value::Path(path.clone())]);
                    let (response, uris) = if title == "cancel" {
                        (1, Vec::new())
                    } else {
                        (0, vec![Value::Str("file:///v/a%20b.mkv".into())])
                    };
                    let signal =
                        Message::signal(&path, "org.freedesktop.portal.Request", "Response")
                            .with_body(vec![
                                Value::U32(response),
                                Value::dict(&[("uris", Value::Array("s".into(), uris))]),
                            ]);
                    if conn.send(&reply).is_err() || conn.send(&signal).is_err() {
                        break;
                    }
                    continue;
                }
                Some("Inhibit") => {
                    let [
                        Value::Str(window),
                        Value::U32(flags),
                        Value::Array(_, options),
                    ] = &msg.body[..]
                    else {
                        panic!("Inhibit with body {:?}", msg.body);
                    };
                    let reason = options
                        .iter()
                        .find_map(|e| match e {
                            Value::Entry(k, v) if **k == Value::Str("reason".into()) => {
                                match v.as_ref() {
                                    Value::Variant(s) => match s.as_ref() {
                                        Value::Str(s) => Some(s.clone()),
                                        _ => None,
                                    },
                                    _ => None,
                                }
                            }
                            _ => None,
                        })
                        .unwrap_or_default();
                    requests += 1;
                    let path = format!("/org/freedesktop/portal/desktop/request/1_9/t{requests}");
                    let _ = tx.send(Seen::Inhibit {
                        window: window.clone(),
                        flags: *flags,
                        reason,
                    });
                    Message::reply_to(&msg, vec![Value::Path(path)])
                }
                Some("Close") => {
                    let _ = tx.send(Seen::Close {
                        path: msg.path.clone().unwrap_or_default(),
                    });
                    Message::reply_to(&msg, Vec::new())
                }
                _ => continue,
            };
            if conn.send(&answer).is_err() {
                break;
            }
        }
    });
    rx
}

fn next(rx: &mpsc::Receiver<Seen>) -> Seen {
    rx.recv_timeout(Duration::from_secs(5))
        .expect("the portal received nothing")
}

#[test]
fn the_client_talks_to_a_real_bus() {
    let bus = Bus::start();
    let mut a = bus.connect();
    let b = bus.connect();
    assert!(a.unique_name().starts_with(':'));
    assert_ne!(a.unique_name(), b.unique_name());
    // a call to a name nobody owns comes back as an error, not a hang
    let err = a
        .call(Message::call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Inhibit",
            "Inhibit",
        ))
        .unwrap_err();
    assert!(err.to_string().contains("ServiceUnknown"), "{err}");
}

// T-PLY-013
#[test]
fn the_portal_sees_inhibit_and_close() {
    let bus = Bus::start();
    let seen = spy_portal(bus.connect());
    let address = bus.address.clone();
    let inhibitor =
        PortalInhibitor::with_bus(move || Connection::open(&address), "Playing a video".into());
    inhibitor.set(true);
    assert_eq!(
        next(&seen),
        Seen::Inhibit {
            window: String::new(),
            flags: 8,
            reason: "Playing a video".into()
        }
    );
    inhibitor.set(true); // already inhibited: nothing is sent
    inhibitor.set(false);
    assert_eq!(
        next(&seen),
        Seen::Close {
            path: "/org/freedesktop/portal/desktop/request/1_9/t1".into()
        }
    );
    inhibitor.set(true);
    assert!(matches!(next(&seen), Seen::Inhibit { .. }));
}

#[test]
fn without_a_portal_the_inhibitor_keeps_working_once_one_appears() {
    let bus = Bus::start();
    let address = bus.address.clone();
    let inhibitor =
        PortalInhibitor::with_bus(move || Connection::open(&address), "Playing a video".into());
    inhibitor.set(true); // no portal: logged, nothing else
    std::thread::sleep(Duration::from_millis(300));
    let seen = spy_portal(bus.connect());
    inhibitor.set(false); // not inhibited: nothing to close
    inhibitor.set(true);
    assert!(matches!(next(&seen), Seen::Inhibit { .. }));
}

// T-LIB-001: the file dialog through the portal
#[test]
fn the_file_chooser_gives_the_chosen_file_or_nothing() {
    let bus = Bus::start();
    let seen = spy_portal(bus.connect());
    let mut conn = bus.connect();
    let dialog = |title: &str| FileDialog {
        dialog: Dialog::Video,
        title: title.into(),
        filters: vec![("Video".into(), vec!["*.mkv"])],
    };
    let path = nikomiru_app::portal::choose(&mut conn, &dialog("Open a video")).unwrap();
    assert_eq!(path, Some(PathBuf::from("/v/a b.mkv")));
    let Seen::OpenFile { title, token } = next(&seen) else {
        panic!()
    };
    assert_eq!(title, "Open a video");
    assert!(token.starts_with("nikomiru"), "{token}");
    // cancelled: nothing, and the connection still works
    let path = nikomiru_app::portal::choose(&mut conn, &dialog("cancel")).unwrap();
    assert_eq!(path, None);
    assert!(matches!(next(&seen), Seen::OpenFile { .. }));
}
