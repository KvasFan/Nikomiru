// SPDX-License-Identifier: GPL-3.0-only

//! The desktop's light or dark preference (SET-003): `color-scheme` of the
//! XDG settings portal's `org.freedesktop.appearance`, read at start and
//! followed through its `SettingChanged` signal.

use std::io;

use crate::dbus::{Connection, Kind, Message, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// Follows the preference on a thread of its own, on the bus that `connect`
/// opens: `tell` gets it at once and at every change, `Some(true)` for dark,
/// `Some(false)` for light, `None` for no preference or no portal.
pub fn follow(
    connect: impl FnOnce() -> io::Result<Connection> + Send + 'static,
    tell: impl Fn(Option<bool>) + Send + 'static,
) {
    let spawned = std::thread::Builder::new()
        .name("appearance".into())
        .spawn(move || {
            if let Err(e) = watch(connect, &tell) {
                tracing::info!("the desktop's color scheme cannot be followed: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("appearance thread: {e}");
    }
}

fn watch(
    connect: impl FnOnce() -> io::Result<Connection>,
    tell: &dyn Fn(Option<bool>),
) -> io::Result<()> {
    let mut conn = connect()?;
    // the changes first: none is lost between them and the reading
    conn.call(
        Message::call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
        )
        .with_body(vec![Value::Str(format!(
            "type='signal',interface='{SETTINGS}',member='SettingChanged',arg0='{NAMESPACE}'"
        ))]),
    )?;
    tell(read(&mut conn).unwrap_or_else(|e| {
        tracing::info!("no color scheme from the portal: {e}");
        None
    }));
    conn.set_timeout(None)?;
    loop {
        let msg = conn.receive()?;
        if msg.kind != Kind::Signal
            || msg.interface.as_deref() != Some(SETTINGS)
            || msg.member.as_deref() != Some("SettingChanged")
        {
            continue;
        }
        if let [Value::Str(namespace), Value::Str(key), value] = &msg.body[..]
            && namespace == NAMESPACE
            && key == KEY
        {
            tell(scheme(value));
        }
    }
}

fn read(conn: &mut Connection) -> io::Result<Option<bool>> {
    let ask = |member: &str| {
        Message::call(PORTAL, PATH, SETTINGS, member)
            .with_body(vec![Value::Str(NAMESPACE.into()), Value::Str(KEY.into())])
    };
    // `ReadOne` is version 2 of the portal; `Read`, older, wraps the value
    // in one more variant
    let reply = match conn.call(ask("ReadOne")) {
        Ok(reply) => reply,
        Err(_) => conn.call(ask("Read"))?,
    };
    Ok(reply.body.first().and_then(scheme))
}

/// `1`: dark preferred; `2`: light; `0`: no preference.
fn scheme(value: &Value) -> Option<bool> {
    match value {
        Value::U32(1) => Some(true),
        Value::U32(2) => Some(false),
        Value::Variant(inner) => scheme(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_values_of_the_portal() {
        let v = |x| Value::Variant(Box::new(x));
        assert_eq!(scheme(&v(Value::U32(1))), Some(true));
        assert_eq!(scheme(&v(v(Value::U32(2)))), Some(false));
        assert_eq!(scheme(&v(Value::U32(0))), None);
        assert_eq!(scheme(&Value::Str("dark".into())), None);
    }
}
