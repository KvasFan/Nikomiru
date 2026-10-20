// SPDX-License-Identifier: GPL-3.0-only

//! Keyboard shortcuts (KEY-001…KEY-004, SPECIFICATION §6.3).
//!
//! Keys are identified by their physical position (KEY-002): Qt's
//! `nativeScanCode` is the XKB keycode, i.e. the evdev code + 8, which does not
//! depend on the layout or the input method. Keys are named with the W3C
//! `KeyboardEvent.code` names (`KeyQ`, `ArrowLeft`…), so `config.toml` stays
//! readable, and a chord is written `Ctrl+Alt+Shift+Key`.

use std::collections::BTreeMap;

/// Where a shortcut applies (KEY-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Context {
    /// Everywhere.
    Global,
    /// The library page.
    Library,
    /// The player page.
    Player,
    /// A dictionary popup.
    Popup,
}

/// An action that a shortcut triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Play/pause.
    PlayPause,
    /// Seek back by the seek step.
    SeekBack,
    /// Seek forward by the seek step.
    SeekForward,
    /// Volume up.
    VolumeUp,
    /// Volume down.
    VolumeDown,
    /// Mute/unmute.
    Mute,
    /// Faster.
    SpeedUp,
    /// Slower.
    SpeedDown,
    /// Normal speed.
    SpeedReset,
    /// 2× while held (PLY-002).
    HoldSpeed,
    /// Previous frame.
    FramePrev,
    /// Next frame.
    FrameNext,
    /// Fullscreen on/off.
    Fullscreen,
    /// Previous chapter.
    ChapterPrev,
    /// Next chapter.
    ChapterNext,
    /// Back: closes a popup, leaves fullscreen, goes back to the library.
    Back,
    /// Open a file.
    OpenFile,
    /// Previous subtitle cue (SUB-012).
    SubPrev,
    /// Next subtitle cue (SUB-012).
    SubNext,
    /// Replay the current cue (SUB-013).
    ReplayCue,
    /// Replay the previous cue.
    ReplayPrevCue,
    /// Subtitle list panel (SUB-015).
    SubList,
    /// Search in the subtitle list (SUB-015).
    SubSearch,
    /// Blur the subtitle on/off (SUB-014).
    SubBlur,
    /// Hide the subtitle on/off (SUB-014).
    SubHide,
    /// Subtitles off/on, keeping the chosen track (SUB-014).
    SubToggle,
    /// Subtitle offset −100 ms (SUB-011).
    SubDelayMinus,
    /// Subtitle offset +100 ms (SUB-011).
    SubDelayPlus,
    /// Align the previous cue to the position (SUB-016).
    SubAlignPrev,
    /// Align the next cue to the position (SUB-016).
    SubAlignNext,
    /// The next entry of the popup (POP-014).
    PopupNext,
    /// The previous entry of the popup (POP-014).
    PopupPrev,
}

impl Action {
    /// Every action, for the editor and the tests.
    pub const ALL: [Action; 32] = [
        Action::PlayPause,
        Action::SeekBack,
        Action::SeekForward,
        Action::VolumeUp,
        Action::VolumeDown,
        Action::Mute,
        Action::SpeedUp,
        Action::SpeedDown,
        Action::SpeedReset,
        Action::HoldSpeed,
        Action::FramePrev,
        Action::FrameNext,
        Action::Fullscreen,
        Action::ChapterPrev,
        Action::ChapterNext,
        Action::Back,
        Action::OpenFile,
        Action::SubPrev,
        Action::SubNext,
        Action::ReplayCue,
        Action::ReplayPrevCue,
        Action::SubList,
        Action::SubSearch,
        Action::SubBlur,
        Action::SubHide,
        Action::SubToggle,
        Action::SubDelayMinus,
        Action::SubDelayPlus,
        Action::SubAlignPrev,
        Action::SubAlignNext,
        Action::PopupNext,
        Action::PopupPrev,
    ];

    /// Name used in `config.toml` (`[shortcuts]`) and in the QML layer.
    pub fn name(self) -> &'static str {
        match self {
            Action::PlayPause => "play_pause",
            Action::SeekBack => "seek_back",
            Action::SeekForward => "seek_forward",
            Action::VolumeUp => "volume_up",
            Action::VolumeDown => "volume_down",
            Action::Mute => "mute",
            Action::SpeedUp => "speed_up",
            Action::SpeedDown => "speed_down",
            Action::SpeedReset => "speed_reset",
            Action::HoldSpeed => "hold_speed",
            Action::FramePrev => "frame_prev",
            Action::FrameNext => "frame_next",
            Action::Fullscreen => "fullscreen",
            Action::ChapterPrev => "chapter_prev",
            Action::ChapterNext => "chapter_next",
            Action::Back => "back",
            Action::OpenFile => "open_file",
            Action::SubPrev => "sub_prev",
            Action::SubNext => "sub_next",
            Action::ReplayCue => "replay_cue",
            Action::ReplayPrevCue => "replay_prev_cue",
            Action::SubList => "sub_list",
            Action::SubSearch => "sub_search",
            Action::SubBlur => "sub_blur",
            Action::SubHide => "sub_hide",
            Action::SubToggle => "sub_toggle",
            Action::SubDelayMinus => "sub_delay_minus",
            Action::SubDelayPlus => "sub_delay_plus",
            Action::SubAlignPrev => "sub_align_prev",
            Action::SubAlignNext => "sub_align_next",
            Action::PopupNext => "popup_next",
            Action::PopupPrev => "popup_prev",
        }
    }

    /// The action with this name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The context of the action.
    pub fn context(self) -> Context {
        match self {
            Action::OpenFile => Context::Global,
            Action::PopupNext | Action::PopupPrev => Context::Popup,
            _ => Context::Player,
        }
    }

    /// Default keys (SPECIFICATION §6.3, M1 and M2 rows).
    pub fn default_keys(self) -> &'static [&'static str] {
        match self {
            Action::PlayPause => &["Space", "KeyP", "MediaPlayPause"],
            Action::SeekBack => &["ArrowLeft", "KeyA"],
            Action::SeekForward => &["ArrowRight", "KeyD"],
            Action::VolumeUp => &["ArrowUp", "Digit0"],
            Action::VolumeDown => &["ArrowDown", "Digit9"],
            Action::Mute => &["KeyM"],
            Action::SpeedUp => &["BracketRight", "Equal"],
            Action::SpeedDown => &["BracketLeft", "Minus"],
            Action::SpeedReset => &["Backspace"],
            Action::HoldSpeed => &["KeyE"],
            Action::FramePrev => &["Comma"],
            Action::FrameNext => &["Period"],
            Action::Fullscreen => &["KeyF", "F11"],
            Action::ChapterPrev => &["PageUp"],
            Action::ChapterNext => &["PageDown"],
            Action::Back => &["Escape", "Alt+ArrowLeft"],
            Action::OpenFile => &["Ctrl+KeyO"],
            Action::SubPrev => &["Ctrl+ArrowLeft"],
            Action::SubNext => &["Ctrl+ArrowRight"],
            Action::ReplayCue => &["KeyR"],
            Action::ReplayPrevCue => &["Shift+KeyR"],
            Action::SubList => &["KeyL"],
            Action::SubSearch => &["Ctrl+KeyF"],
            Action::SubBlur => &["KeyB"],
            Action::SubHide => &["KeyH"],
            Action::SubToggle => &["KeyV"],
            Action::SubDelayMinus => &["KeyZ"],
            Action::SubDelayPlus => &["KeyX"],
            Action::SubAlignPrev => &["Ctrl+Shift+ArrowLeft"],
            Action::SubAlignNext => &["Ctrl+Shift+ArrowRight"],
            Action::PopupNext => &["Tab"],
            Action::PopupPrev => &["Shift+Tab"],
        }
    }
}

/// Letter keys: evdev code and W3C `code` name.
const LETTERS: [(u32, &str); 26] = [
    (16, "KeyQ"),
    (17, "KeyW"),
    (18, "KeyE"),
    (19, "KeyR"),
    (20, "KeyT"),
    (21, "KeyY"),
    (22, "KeyU"),
    (23, "KeyI"),
    (24, "KeyO"),
    (25, "KeyP"),
    (30, "KeyA"),
    (31, "KeyS"),
    (32, "KeyD"),
    (33, "KeyF"),
    (34, "KeyG"),
    (35, "KeyH"),
    (36, "KeyJ"),
    (37, "KeyK"),
    (38, "KeyL"),
    (44, "KeyZ"),
    (45, "KeyX"),
    (46, "KeyC"),
    (47, "KeyV"),
    (48, "KeyB"),
    (49, "KeyN"),
    (50, "KeyM"),
];

/// The other keys with a name: evdev code and W3C `code` name.
const OTHERS: [(u32, &str); 49] = [
    (1, "Escape"),
    (2, "Digit1"),
    (3, "Digit2"),
    (4, "Digit3"),
    (5, "Digit4"),
    (6, "Digit5"),
    (7, "Digit6"),
    (8, "Digit7"),
    (9, "Digit8"),
    (10, "Digit9"),
    (11, "Digit0"),
    (12, "Minus"),
    (13, "Equal"),
    (14, "Backspace"),
    (15, "Tab"),
    (26, "BracketLeft"),
    (27, "BracketRight"),
    (28, "Enter"),
    (39, "Semicolon"),
    (40, "Quote"),
    (41, "Backquote"),
    (43, "Backslash"),
    (51, "Comma"),
    (52, "Period"),
    (53, "Slash"),
    (57, "Space"),
    (59, "F1"),
    (60, "F2"),
    (61, "F3"),
    (62, "F4"),
    (63, "F5"),
    (64, "F6"),
    (65, "F7"),
    (66, "F8"),
    (67, "F9"),
    (68, "F10"),
    (87, "F11"),
    (88, "F12"),
    (102, "Home"),
    (103, "ArrowUp"),
    (104, "PageUp"),
    (105, "ArrowLeft"),
    (106, "ArrowRight"),
    (107, "End"),
    (108, "ArrowDown"),
    (109, "PageDown"),
    (110, "Insert"),
    (111, "Delete"),
    (164, "MediaPlayPause"),
];

/// W3C `code` name of an evdev key code.
pub fn code_name(evdev: u32) -> Option<&'static str> {
    LETTERS
        .iter()
        .chain(OTHERS.iter())
        .find(|(c, _)| *c == evdev)
        .map(|(_, n)| *n)
}

/// The evdev code of a W3C `code` name (the reverse of [`code_name`]).
pub fn evdev_code(name: &str) -> Option<u32> {
    LETTERS
        .iter()
        .chain(OTHERS.iter())
        .find(|(_, n)| *n == name)
        .map(|(c, _)| *c)
}

/// Modifier keys of a chord.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Control.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
}

impl Modifiers {
    fn any(self) -> bool {
        self.ctrl || self.alt || self.shift
    }
}

/// The chord of a key event: the physical key as a Linux evdev code (what winit
/// reports on Linux, KEY-002) and the modifiers. `None` for keys without a name
/// or for a bare modifier.
pub fn chord(evdev: u32, mods: Modifiers) -> Option<String> {
    let name = code_name(evdev)?;
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("Ctrl+");
    }
    if mods.alt {
        s.push_str("Alt+");
    }
    if mods.shift {
        s.push_str("Shift+");
    }
    s.push_str(name);
    Some(s)
}

/// Normalizes a chord written by hand (`shift+ctrl+keyq` → `Ctrl+Shift+KeyQ`).
pub fn normalize(chord: &str) -> String {
    let mut mods = Modifiers::default();
    let mut key = "";
    for part in chord.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "alt" => mods.alt = true,
            "shift" => mods.shift = true,
            _ => key = part,
        }
    }
    let key = (1..=255)
        .filter_map(code_name)
        .find(|n| n.eq_ignore_ascii_case(key))
        .unwrap_or(key);
    let mut s = String::new();
    if mods.ctrl {
        s.push_str("Ctrl+");
    }
    if mods.alt {
        s.push_str("Alt+");
    }
    if mods.shift {
        s.push_str("Shift+");
    }
    s.push_str(key);
    s
}

/// Two actions of the same context bound to the same chord (KEY-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The chord.
    pub chord: String,
    /// The actions that want it.
    pub actions: Vec<Action>,
}

/// The bindings in use: defaults plus the user's overrides (KEY-001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    by_chord: BTreeMap<(Context, String), Action>,
    keys: BTreeMap<Action, Vec<String>>,
}

impl Bindings {
    /// The default bindings.
    pub fn defaults() -> Self {
        match Self::with_overrides(&BTreeMap::new()) {
            Ok(b) => b,
            Err(_) => Self {
                by_chord: BTreeMap::new(),
                keys: BTreeMap::new(),
            },
        }
    }

    /// Defaults replaced, action by action, by `[shortcuts]` of `config.toml`.
    /// Unknown action names are ignored. Conflicts in a context are an error.
    pub fn with_overrides(
        overrides: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, Vec<Conflict>> {
        let mut keys = BTreeMap::new();
        for action in Action::ALL {
            let list: Vec<String> = match overrides.get(action.name()) {
                Some(v) => v.iter().map(|c| normalize(c)).collect(),
                None => action
                    .default_keys()
                    .iter()
                    .map(|c| (*c).to_string())
                    .collect(),
            };
            keys.insert(action, list);
        }
        let mut owners: BTreeMap<(Context, String), Vec<Action>> = BTreeMap::new();
        for (action, list) in &keys {
            for chord in list {
                owners
                    .entry((action.context(), chord.clone()))
                    .or_default()
                    .push(*action);
            }
        }
        let conflicts: Vec<Conflict> = owners
            .iter()
            .filter(|(_, a)| a.len() > 1)
            .map(|((_, chord), a)| Conflict {
                chord: chord.clone(),
                actions: a.clone(),
            })
            .collect();
        if !conflicts.is_empty() {
            return Err(conflicts);
        }
        let by_chord = owners.into_iter().map(|(k, a)| (k, a[0])).collect();
        Ok(Self { by_chord, keys })
    }

    /// The keys of an action.
    pub fn keys(&self, action: Action) -> &[String] {
        self.keys.get(&action).map_or(&[], Vec::as_slice)
    }

    /// The action for a chord in `context` (global shortcuts apply everywhere).
    /// With a text field focused, chords without modifiers do nothing (KEY-004).
    pub fn action(&self, context: Context, chord: &str, text_focus: bool) -> Option<Action> {
        if text_focus && !chord.contains('+') {
            return None;
        }
        self.by_chord
            .get(&(context, chord.to_string()))
            .or_else(|| self.by_chord.get(&(Context::Global, chord.to_string())))
            .copied()
    }
}

/// Bare modifiers have no chord: callers can use this to ignore them early.
pub fn is_modifier_only(mods: Modifiers, native_scan_code: u32) -> bool {
    matches!(
        native_scan_code.saturating_sub(8),
        29 | 42 | 54 | 56 | 97 | 100 | 125 | 126
    ) && mods.any()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO: Modifiers = Modifiers {
        ctrl: false,
        alt: false,
        shift: false,
    };

    // T-KEY-002
    #[test]
    fn keys_are_identified_by_scan_code() {
        // evdev KEY_Q = 16: "KeyQ" whatever the layout prints (on AZERTY that
        // key types "a")
        assert_eq!(chord(16, NO).as_deref(), Some("KeyQ"));
        assert_eq!(chord(57, NO).as_deref(), Some("Space"));
        let ctrl = Modifiers { ctrl: true, ..NO };
        assert_eq!(chord(105, ctrl).as_deref(), Some("Ctrl+ArrowLeft"));
        // reserved code, bare Ctrl
        assert_eq!(chord(0, NO), None);
        assert_eq!(chord(29, ctrl), None);
        assert_eq!(normalize("shift+ctrl+keyq"), "Ctrl+Shift+KeyQ");
    }

    // T-KEY-001
    #[test]
    fn remapping_replaces_the_defaults_of_one_action() {
        let mut overrides = BTreeMap::new();
        overrides.insert("play_pause".to_string(), vec!["KeyK".to_string()]);
        overrides.insert("not_an_action".to_string(), vec!["KeyZ".to_string()]);
        let b = Bindings::with_overrides(&overrides).unwrap();
        assert_eq!(
            b.action(Context::Player, "KeyK", false),
            Some(Action::PlayPause)
        );
        assert_eq!(b.action(Context::Player, "Space", false), None);
        assert_eq!(b.action(Context::Player, "KeyM", false), Some(Action::Mute));
        // the global "open file" works in the player too
        assert_eq!(
            b.action(Context::Player, "Ctrl+KeyO", false),
            Some(Action::OpenFile)
        );
        // round trip through config.toml
        let cfg = nikomiru_core::Config {
            shortcuts: overrides,
            ..Default::default()
        };
        let text = toml_round_trip(&cfg);
        let again = Bindings::with_overrides(&text.shortcuts).unwrap();
        assert_eq!(again, b);
    }

    fn toml_round_trip(cfg: &nikomiru_core::Config) -> nikomiru_core::Config {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        cfg.save(&path).unwrap();
        nikomiru_core::Config::load(&path).unwrap()
    }

    // T-KEY-003
    #[test]
    fn conflicts_in_the_same_context_are_rejected() {
        let mut overrides = BTreeMap::new();
        overrides.insert("mute".to_string(), vec!["Space".to_string()]);
        let err = Bindings::with_overrides(&overrides).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].chord, "Space");
        assert_eq!(err[0].actions, [Action::PlayPause, Action::Mute]);
        assert!(
            Bindings::with_overrides(&BTreeMap::new()).is_ok(),
            "the defaults have no conflicts"
        );
    }

    // T-KEY-004
    #[test]
    fn single_keys_are_off_in_a_text_field() {
        let b = Bindings::defaults();
        assert_eq!(b.action(Context::Player, "Space", true), None);
        assert_eq!(
            b.action(Context::Player, "Space", false),
            Some(Action::PlayPause)
        );
        assert_eq!(
            b.action(Context::Player, "Ctrl+KeyO", true),
            Some(Action::OpenFile)
        );
    }

    #[test]
    fn every_default_key_has_a_known_name() {
        for a in Action::ALL {
            assert_eq!(Action::from_name(a.name()), Some(a));
            for k in a.default_keys() {
                let key = k.rsplit('+').next().unwrap();
                assert!((1..=255).filter_map(code_name).any(|n| n == key), "{k}");
            }
        }
    }
}
