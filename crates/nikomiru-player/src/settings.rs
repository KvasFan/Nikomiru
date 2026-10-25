// SPDX-License-Identifier: GPL-3.0-only

//! The player's option set (PLY-016, PLY-018): the options the Player
//! settings show as controls, the raw options, and presets in `mpv.conf`
//! format, loaded and saved.

use std::collections::BTreeMap;

use nikomiru_core::config::PlayerConfig;

use crate::options::{Line, parse_mpv_conf};

/// The groups of the Player settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Scaling, debanding, interpolation, HDR, the picture.
    Rendering,
    /// Audio.
    Audio,
    /// Hardware decoding.
    Advanced,
}

/// How a control edits its option.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// One of these values.
    Choice(&'static [&'static str]),
    /// `yes` or `no`.
    Flag,
    /// A whole number between the bounds.
    Integer(i64, i64),
    /// Free text (`target-peak`: `auto` or nits).
    Text,
}

/// A control of the Player settings (PLY-018): the mpv option it writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Control {
    /// The option.
    pub key: &'static str,
    /// Its group.
    pub group: Group,
    /// How it is edited.
    pub kind: Kind,
    /// mpv takes it only from the next file: a profile, the decoder.
    pub next_file: bool,
}

const SCALERS: &[&str] = &[
    "bilinear",
    "bicubic",
    "hermite",
    "mitchell",
    "catmull_rom",
    "spline36",
    "lanczos",
    "ewa_lanczos",
    "ewa_lanczossharp",
];

const fn control(key: &'static str, group: Group, kind: Kind) -> Control {
    Control {
        key,
        group,
        kind,
        next_file: false,
    }
}

/// The controls, in the order the page shows them (PLY-018).
pub const CONTROLS: [Control; 24] = [
    Control {
        key: "profile",
        group: Group::Rendering,
        kind: Kind::Choice(&["default", "fast", "high-quality"]),
        next_file: true,
    },
    control("scale", Group::Rendering, Kind::Choice(SCALERS)),
    control("cscale", Group::Rendering, Kind::Choice(SCALERS)),
    control("dscale", Group::Rendering, Kind::Choice(SCALERS)),
    control("deband", Group::Rendering, Kind::Flag),
    control("deband-iterations", Group::Rendering, Kind::Integer(1, 16)),
    control("deband-threshold", Group::Rendering, Kind::Integer(0, 4096)),
    control(
        "dither-depth",
        Group::Rendering,
        Kind::Choice(&["auto", "no", "8", "10"]),
    ),
    control("interpolation", Group::Rendering, Kind::Flag),
    control(
        "tscale",
        Group::Rendering,
        Kind::Choice(&["oversample", "linear", "catmull_rom", "mitchell", "sphinx"]),
    ),
    control(
        "video-sync",
        Group::Rendering,
        Kind::Choice(&[
            "audio",
            "display-resample",
            "display-vdrop",
            "display-adrop",
        ]),
    ),
    control(
        "tone-mapping",
        Group::Rendering,
        Kind::Choice(&[
            "auto", "clip", "mobius", "reinhard", "hable", "bt.2390", "bt.2446a", "spline",
        ]),
    ),
    control("target-peak", Group::Rendering, Kind::Text),
    control(
        "hdr-compute-peak",
        Group::Rendering,
        Kind::Choice(&["auto", "yes", "no"]),
    ),
    control(
        "deinterlace",
        Group::Rendering,
        Kind::Choice(&["no", "yes", "auto"]),
    ),
    control("brightness", Group::Rendering, Kind::Integer(-100, 100)),
    control("contrast", Group::Rendering, Kind::Integer(-100, 100)),
    control("saturation", Group::Rendering, Kind::Integer(-100, 100)),
    control("gamma", Group::Rendering, Kind::Integer(-100, 100)),
    control(
        "audio-channels",
        Group::Audio,
        Kind::Choice(&["auto-safe", "auto", "stereo", "5.1", "7.1,5.1,stereo"]),
    ),
    control("audio-pitch-correction", Group::Audio, Kind::Flag),
    control("volume-max", Group::Audio, Kind::Integer(100, 1000)),
    control("audio-normalize-downmix", Group::Audio, Kind::Flag),
    Control {
        key: "hwdec",
        group: Group::Advanced,
        kind: Kind::Choice(&["auto-safe", "auto-copy-safe", "no"]),
        next_file: true,
    },
];

/// The control of `key`.
pub fn control_of(key: &str) -> Option<&'static Control> {
    CONTROLS.iter().find(|c| c.key == key)
}

/// The value of a setting that is not written to a preset: Nikomiru's own
/// defaults for `hwdec` and the profile (PLY-006).
fn nikomiru_default(key: &str) -> Option<&'static str> {
    match key {
        "hwdec" => Some("auto-safe"),
        "profile" => Some("default"),
        _ => None,
    }
}

/// The option set (PLY-016): the options set from the controls (and any other
/// set the same way, such as `ao` in the tests), then the raw options, which
/// win.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptionSet {
    /// `hwdec`, `profile` and the options of the controls; other options set
    /// as `[player.mpv_options]` stay here too.
    pub values: BTreeMap<String, String>,
    /// The raw options, `mpv.conf` text.
    pub raw: String,
}

impl OptionSet {
    /// The option set of the settings.
    pub fn from_config(cfg: &PlayerConfig) -> Self {
        let mut values = cfg.mpv_options.clone();
        values.insert("hwdec".into(), cfg.hwdec.clone());
        values.insert("profile".into(), cfg.quality_profile.clone());
        Self {
            values,
            raw: cfg.raw_mpv_options.clone(),
        }
    }

    /// Writes the set into the settings.
    pub fn apply_to(&self, cfg: &mut PlayerConfig) {
        let mut values = self.values.clone();
        cfg.hwdec = values.remove("hwdec").unwrap_or_else(|| "auto-safe".into());
        cfg.quality_profile = values.remove("profile").unwrap_or_else(|| "default".into());
        cfg.mpv_options = values;
        cfg.raw_mpv_options.clone_from(&self.raw);
    }

    /// A set loaded from `mpv.conf` text (PLY-016): the options the controls
    /// show fill them, the others become the raw options; the report of every
    /// line, for the preview.
    pub fn from_mpv_conf(text: &str) -> (Self, Vec<Line>) {
        let lines = parse_mpv_conf(text);
        let mut set = Self::default();
        let mut raw = Vec::new();
        for line in &lines {
            if let Line::Apply { option, .. } = line {
                if control_of(&option.key).is_some() {
                    set.values.insert(option.key.clone(), option.value.clone());
                } else {
                    raw.push(format!("{}={}", option.key, quoted(&option.value)));
                }
            }
        }
        for (key, value) in [("hwdec", "auto-safe"), ("profile", "default")] {
            set.values.entry(key.into()).or_insert_with(|| value.into());
        }
        set.raw = raw.join("\n");
        (set, lines)
    }

    /// The set as a preset (PLY-016): `header` as a comment, then only the
    /// options that differ from the defaults, so the file also works in mpv.
    pub fn to_mpv_conf(&self, header: &str) -> String {
        let mut out = String::new();
        for line in header.lines() {
            out.push_str("# ");
            out.push_str(line);
            out.push('\n');
        }
        for (key, value) in &self.values {
            if nikomiru_default(key) == Some(value.as_str()) {
                continue;
            }
            out.push_str(&format!("{key}={}\n", quoted(value)));
        }
        let raw = self.raw.trim();
        if !raw.is_empty() {
            out.push_str(raw);
            out.push('\n');
        }
        out
    }
}

/// A value as `mpv.conf` reads it back: quoted when it holds a space or `#`.
fn quoted(value: &str) -> String {
    if value.contains([' ', '#', '"']) || value.is_empty() {
        format!("\"{}\"", value.replace('"', "'"))
    } else {
        value.to_string()
    }
}

/// A preset name as a file name: letters, digits, `-`, `_`, `.` and spaces;
/// `None` for an empty or hidden name.
pub fn preset_file_name(name: &str) -> Option<String> {
    let name = name.trim();
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || " -_.".contains(c));
    ok.then(|| format!("{name}.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Ignored;

    // T-PLY-016
    #[test]
    fn a_preset_fills_the_controls_and_the_rest_is_raw() {
        let text = "\
# my mpv.conf
profile=high-quality
scale=ewa_lanczossharp
deband
--glsl-shaders=\"~/shaders/a b.glsl\"
script=evil.lua
[anime]
nonsense line!
";
        let (set, lines) = OptionSet::from_mpv_conf(text);
        assert_eq!(set.values["profile"], "high-quality");
        assert_eq!(set.values["scale"], "ewa_lanczossharp");
        assert_eq!(set.values["deband"], "yes");
        assert_eq!(set.values["hwdec"], "auto-safe");
        assert_eq!(set.raw, "glsl-shaders=\"~/shaders/a b.glsl\"");
        assert!(lines.iter().any(|l| matches!(
            l,
            Line::Ignore {
                reason: Ignored::Blocked(_),
                ..
            }
        )));
        assert!(lines.iter().any(|l| matches!(
            l,
            Line::Ignore {
                reason: Ignored::Section,
                ..
            }
        )));
        assert!(
            lines
                .iter()
                .any(|l| matches!(l, Line::Invalid { line: 8, .. }))
        );
        // saved: a header, only what differs from the defaults, read back the same
        let saved = set.to_mpv_conf("Nikomiru 0.1.0\n2026-10-25");
        assert!(
            saved.starts_with("# Nikomiru 0.1.0\n# 2026-10-25\n"),
            "{saved}"
        );
        assert!(!saved.contains("hwdec"), "{saved}");
        let (again, _) = OptionSet::from_mpv_conf(&saved);
        assert_eq!(again, set);
    }

    // T-PLY-016
    #[test]
    fn the_settings_and_the_set_agree() {
        let mut cfg = PlayerConfig::default();
        cfg.mpv_options.insert("ao".into(), "null".into());
        cfg.mpv_options.insert("deband".into(), "yes".into());
        cfg.raw_mpv_options = "sharpen=0.5".into();
        let set = OptionSet::from_config(&cfg);
        assert_eq!(set.values["hwdec"], "auto-safe");
        let mut back = PlayerConfig::default();
        set.apply_to(&mut back);
        assert_eq!(back.mpv_options, cfg.mpv_options);
        assert_eq!(back.raw_mpv_options, "sharpen=0.5");
        assert_eq!(
            (back.hwdec.as_str(), back.quality_profile.as_str()),
            ("auto-safe", "default")
        );
        assert_eq!(preset_file_name(" Anime HQ "), Some("Anime HQ.conf".into()));
        for bad in ["", ".hidden", "a/b", "../x"] {
            assert_eq!(preset_file_name(bad), None, "{bad}");
        }
    }

    // T-PLY-018: every control's option can be set from a preset
    #[test]
    fn every_control_is_an_option_mpv_conf_can_hold() {
        for c in CONTROLS {
            assert!(crate::options::classify(c.key).is_none(), "{}", c.key);
            if let Kind::Choice(values) = c.kind {
                assert!(!values.is_empty());
            }
        }
    }
}
