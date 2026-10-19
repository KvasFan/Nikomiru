// SPDX-License-Identifier: GPL-3.0-only

//! Screensaver inhibition through the XDG desktop portal (PLY-013). A thread
//! owns the bus connection, so the UI never waits for D-Bus; the inhibition
//! lasts until the portal request is closed or the connection drops.

use std::io;
use std::sync::mpsc;

use nikomiru_ui::session::IdleInhibitor;

use crate::dbus::{Connection, Message, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const INHIBIT: &str = "org.freedesktop.portal.Inhibit";
const REQUEST: &str = "org.freedesktop.portal.Request";

/// `Inhibit` flag for idleness: screensaver and screen blanking.
const IDLE: u32 = 8;

/// Asks the portal to keep the screen on.
#[derive(Debug)]
pub struct PortalInhibitor {
    wishes: mpsc::Sender<bool>,
}

impl PortalInhibitor {
    /// Uses the session bus; `reason` is shown by the desktop.
    pub fn start(reason: String) -> Self {
        Self::with_bus(Connection::session, reason)
    }

    /// Uses the bus that `connect` opens (tests use a private bus).
    pub fn with_bus(
        connect: impl FnMut() -> io::Result<Connection> + Send + 'static,
        reason: String,
    ) -> Self {
        let (wishes, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("portal".into())
            .spawn(move || worker(&rx, connect, &reason));
        if let Err(e) = spawned {
            tracing::warn!("portal thread: {e}");
        }
        Self { wishes }
    }
}

impl IdleInhibitor for PortalInhibitor {
    fn set(&self, inhibit: bool) {
        let _ = self.wishes.send(inhibit);
    }
}

fn worker(
    wishes: &mpsc::Receiver<bool>,
    mut connect: impl FnMut() -> io::Result<Connection>,
    reason: &str,
) {
    let mut conn: Option<Connection> = None;
    let mut handle: Option<String> = None;
    let mut warned = false;
    while let Ok(mut want) = wishes.recv() {
        // only the latest wish counts
        while let Ok(w) = wishes.try_recv() {
            want = w;
        }
        if want == handle.is_some() {
            continue;
        }
        let result = if want {
            inhibit(&mut conn, &mut connect, reason).map(|h| handle = Some(h))
        } else {
            release(&mut conn, handle.take())
        };
        match result {
            Ok(()) => {
                warned = false;
                tracing::debug!("screensaver inhibited: {want}");
            }
            Err(e) => {
                // a broken connection also ends the inhibition
                conn = None;
                if !warned {
                    tracing::warn!("screensaver inhibition through the portal: {e}");
                    warned = true;
                }
            }
        }
    }
}

fn inhibit(
    conn: &mut Option<Connection>,
    connect: &mut impl FnMut() -> io::Result<Connection>,
    reason: &str,
) -> io::Result<String> {
    let c = match conn {
        Some(c) => c,
        None => conn.insert(connect()?),
    };
    let reply = c.call(
        Message::call(PORTAL, PORTAL_PATH, INHIBIT, "Inhibit").with_body(vec![
            Value::Str(String::new()),
            Value::U32(IDLE),
            Value::dict(&[("reason", Value::Str(reason.into()))]),
        ]),
    )?;
    match reply.body.first() {
        Some(Value::Path(p)) => Ok(p.clone()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Inhibit: the reply has no request handle",
        )),
    }
}

fn release(conn: &mut Option<Connection>, handle: Option<String>) -> io::Result<()> {
    let (Some(c), Some(handle)) = (conn.as_mut(), handle) else {
        return Ok(());
    };
    c.call(Message::call(PORTAL, &handle, REQUEST, "Close"))
        .map(|_| ())
}
