// SPDX-License-Identifier: GPL-3.0-only

//! Portable shortcut maps (KEY-008…KEY-010): a versioned JSON file of
//! physical-key chords, read into a preview before anything changes.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::shortcuts::{Action, Bindings, Conflict, evdev_code, normalize};

/// The format's name.
pub const FORMAT: &str = "nikomiru-shortcuts";
/// The format's version.
pub const VERSION: u64 = 1;
/// The largest map file.
pub const MAX_BYTES: usize = 64 << 10;
/// Actions in a map at most.
pub const MAX_ACTIONS: usize = 512;
/// Chords of an action at most.
pub const MAX_CHORDS: usize = 8;
/// The longest chord.
pub const MAX_CHORD: usize = 64;

/// The keys of `action` under `overrides`.
pub fn keys_of(overrides: &BTreeMap<String, Vec<String>>, action: Action) -> Vec<String> {
    match overrides.get(action.name()) {
        Some(list) => list.iter().map(|c| normalize(c)).collect(),
        None => action
            .default_keys()
            .iter()
            .map(|c| (*c).to_string())
            .collect(),
    }
}

/// Every action's keys under `overrides`, as a map file (KEY-008).
pub fn export(overrides: &BTreeMap<String, Vec<String>>) -> String {
    let actions: Map<String, Value> = Action::ALL
        .into_iter()
        .map(|a| (a.name().to_string(), json!(keys_of(overrides, a))))
        .collect();
    let map = json!({ "format": FORMAT, "version": VERSION, "actions": actions });
    serde_json::to_string_pretty(&map).unwrap_or_default() + "\n"
}

/// Whether a normalized chord names a physical key Nikomiru knows, with
/// modifiers only before it.
fn known(chord: &str) -> bool {
    let key = chord.rsplit('+').next().unwrap_or_default();
    let mods = chord.matches('+').count();
    chord.len() <= MAX_CHORD
        && evdev_code(key).is_some()
        && chord
            .split('+')
            .take(mods)
            .all(|m| matches!(m, "Ctrl" | "Alt" | "Shift"))
}

/// One action the map changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The action.
    pub action: Action,
    /// Its keys now.
    pub before: Vec<String>,
    /// Its keys from the map.
    pub after: Vec<String>,
}

/// What importing a map would do (KEY-009).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preview {
    /// The actions it changes.
    pub changes: Vec<Change>,
    /// Action names this version does not know: ignored.
    pub unknown: Vec<String>,
    /// Chords that name no key: the action and the chord.
    pub invalid: Vec<(String, String)>,
    /// The conflicts the result would have.
    pub conflicts: Vec<Conflict>,
    /// The overrides the result would have.
    pub overrides: BTreeMap<String, Vec<String>>,
}

impl Preview {
    /// Whether Apply may run (KEY-010).
    pub fn applicable(&self) -> bool {
        self.invalid.is_empty() && self.conflicts.is_empty()
    }
}

/// Reads a map against `current` overrides. Errors are interface string
/// keys; nothing is changed by reading.
pub fn preview(
    bytes: &[u8],
    current: &BTreeMap<String, Vec<String>>,
) -> Result<Preview, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("shortcut-map-too-large");
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "shortcut-map-invalid")?;
    let object = value.as_object().ok_or("shortcut-map-invalid")?;
    if object.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err("shortcut-map-invalid");
    }
    match object.get("version").and_then(Value::as_u64) {
        Some(VERSION) => {}
        Some(v) if v > VERSION => return Err("shortcut-map-newer"),
        _ => return Err("shortcut-map-invalid"),
    }
    let actions = object
        .get("actions")
        .and_then(Value::as_object)
        .ok_or("shortcut-map-invalid")?;
    if actions.len() > MAX_ACTIONS {
        return Err("shortcut-map-too-large");
    }
    let mut out = Preview {
        overrides: current.clone(),
        ..Preview::default()
    };
    for (name, chords) in actions {
        let list = chords.as_array().ok_or("shortcut-map-invalid")?;
        if list.len() > MAX_CHORDS {
            return Err("shortcut-map-too-large");
        }
        let mut keys: Vec<String> = Vec::new();
        for chord in list {
            let chord = chord.as_str().ok_or("shortcut-map-invalid")?;
            if chord.len() > MAX_CHORD {
                return Err("shortcut-map-too-large");
            }
            // only modifiers before the key: normalize would drop others
            let raw_ok = chord.split('+').rev().skip(1).all(|m| {
                matches!(
                    m.to_ascii_lowercase().as_str(),
                    "ctrl" | "control" | "alt" | "shift"
                )
            });
            let chord = normalize(chord);
            if !raw_ok || !known(&chord) {
                out.invalid.push((name.clone(), chord));
            } else if !keys.contains(&chord) {
                keys.push(chord);
            }
        }
        let Some(action) = Action::from_name(name) else {
            out.unknown.push(name.chars().take(MAX_CHORD).collect());
            continue;
        };
        let before = keys_of(current, action);
        if before != keys {
            out.changes.push(Change {
                action,
                before,
                after: keys.clone(),
            });
        }
        if keys
            .iter()
            .map(String::as_str)
            .eq(action.default_keys().iter().copied())
        {
            out.overrides.remove(action.name());
        } else {
            out.overrides.insert(action.name().to_string(), keys);
        }
    }
    out.changes.sort_by_key(|c| c.action);
    if let Err(conflicts) = Bindings::with_overrides(&out.overrides) {
        out.conflicts = conflicts;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::Context;

    // T-KEY-008: an export reads back unchanged.
    #[test]
    fn an_export_reads_back_as_no_change() {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "mute".to_string(),
            vec!["KeyK".to_string(), "Ctrl+KeyM".into()],
        );
        overrides.insert("play_pause".to_string(), vec!["Space".to_string()]);
        let text = export(&overrides);
        assert!(text.contains("\"format\": \"nikomiru-shortcuts\""));
        assert!(text.contains("\"Ctrl+KeyM\""));
        let p = preview(text.as_bytes(), &overrides).unwrap();
        assert!(p.changes.is_empty(), "{:?}", p.changes);
        assert!(p.applicable());
        assert_eq!(
            Bindings::with_overrides(&p.overrides),
            Bindings::with_overrides(&overrides)
        );
        // into a fresh setup it reproduces the same bindings
        let fresh = preview(text.as_bytes(), &BTreeMap::new()).unwrap();
        assert_eq!(
            fresh.changes.len(),
            2,
            "mute and play/pause differ from the defaults"
        );
        assert_eq!(
            Bindings::with_overrides(&fresh.overrides)
                .unwrap()
                .keys(Action::Mute),
            ["KeyK", "Ctrl+KeyM"]
        );
    }

    fn map(actions: &str) -> Vec<u8> {
        format!(r#"{{"format":"nikomiru-shortcuts","version":1,"actions":{{{actions}}}}}"#)
            .into_bytes()
    }

    // T-KEY-009: normalization, duplicates, unknown names, invalid keys and conflicts.
    #[test]
    fn previews_list_what_would_change() {
        let p = preview(
            &map(r#""mute":["shift+keyq","Shift+KeyQ"],"from_the_future":["KeyZ"],"seek_back":["NoSuchKey","Hyper+KeyA"]"#),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(p.unknown, ["from_the_future"]);
        assert_eq!(p.invalid.len(), 2);
        assert!(!p.applicable());
        let mute = p.changes.iter().find(|c| c.action == Action::Mute).unwrap();
        assert_eq!(mute.after, ["Shift+KeyQ"], "normalized once");
        let p = preview(&map(r#""mute":["Space"]"#), &BTreeMap::new()).unwrap();
        assert!(!p.conflicts.is_empty(), "Space is play/pause's");
        assert!(!p.applicable());
        let p = preview(&map(r#""mute":["KeyM"]"#), &BTreeMap::new()).unwrap();
        assert!(
            p.changes.is_empty() && p.overrides.is_empty(),
            "the defaults"
        );
    }

    // T-KEY-009: other formats, newer versions and bounds change nothing.
    #[test]
    fn bad_maps_are_refused() {
        let current = BTreeMap::new();
        assert_eq!(preview(b"{", &current), Err("shortcut-map-invalid"));
        assert_eq!(
            preview(br#"{"format":"other","version":1,"actions":{}}"#, &current),
            Err("shortcut-map-invalid")
        );
        assert_eq!(
            preview(
                br#"{"format":"nikomiru-shortcuts","version":2,"actions":{}}"#,
                &current
            ),
            Err("shortcut-map-newer")
        );
        assert_eq!(
            preview(&vec![b' '; MAX_BYTES + 1], &current),
            Err("shortcut-map-too-large")
        );
        let many = (0..=MAX_CHORDS)
            .map(|i| format!("\"Digit{}\"", i % 10))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            preview(&map(&format!("\"mute\":[{many}]")), &current),
            Err("shortcut-map-too-large")
        );
    }

    // T-KEY-010: imported single keys never fire while typing.
    #[test]
    fn imported_keys_keep_typing_priority() {
        let p = preview(&map(r#""mute":["KeyQ"]"#), &BTreeMap::new()).unwrap();
        let b = Bindings::with_overrides(&p.overrides).unwrap();
        assert_eq!(b.action(Context::Player, "KeyQ", false), Some(Action::Mute));
        assert_eq!(b.action(Context::Player, "KeyQ", true), None);
    }
}
