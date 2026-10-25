// SPDX-License-Identifier: GPL-3.0-only

//! mpv options: the fixed ones (PLY-005), the configurable ones (PLY-006), the
//! hwdec interop rule (SPECIFICATION §5.2.2) and the parser of raw options and
//! presets in `mpv.conf` syntax, with the table of blocked options (PLY-017).

use nikomiru_core::config::PlayerConfig;

/// An mpv option, ready for `mpv_set_option_string` / `mpv_set_property_string`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvOption {
    /// Option name, without `--`.
    pub key: String,
    /// Value as a string (`yes`/`no` for flags).
    pub value: String,
}

impl MpvOption {
    fn new(key: &str, value: &str) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// PLY-005: set before `mpv_initialize`, never changeable.
pub const FIXED: [(&str, &str); 10] = [
    ("vo", "libmpv"),
    ("config", "no"),
    ("input-default-bindings", "no"),
    ("input-vo-keyboard", "no"),
    ("osc", "no"),
    ("terminal", "no"),
    ("idle", "yes"),
    ("keep-open", "yes"),
    ("ytdl", "no"),
    ("sub-auto", "no"),
];

/// Built-in mpv profiles that a preset may select with `profile=`.
const BUILTIN_PROFILES: [&str; 4] = ["default", "fast", "high-quality", "gpu-hq"];

/// Why a line of raw options or of a preset is not applied (PLY-016, PLY-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignored {
    /// One of the fixed options of PLY-005.
    Fixed,
    /// Runs code, loads other configuration, opens sockets or writes files.
    Blocked(&'static str),
    /// No effect in Nikomiru (libmpv render API, text subtitles drawn by Nikomiru).
    NoEffect(&'static str),
    /// `[section]`: user-defined profiles are not supported.
    Section,
}

/// What happens to one line of `mpv.conf` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Applied (mpv may still reject the value: the player reports it per line).
    Apply {
        /// 1-based line number.
        line: usize,
        /// The option.
        option: MpvOption,
    },
    /// Ignored, with the reason.
    Ignore {
        /// 1-based line number.
        line: usize,
        /// Option name or section header.
        name: String,
        /// Why.
        reason: Ignored,
    },
    /// Not valid syntax.
    Invalid {
        /// 1-based line number.
        line: usize,
        /// What is wrong.
        error: String,
    },
}

/// The class of a key: allowed, or ignored for a reason (PLY-017).
pub fn classify(key: &str) -> Option<Ignored> {
    if FIXED.iter().any(|(k, _)| *k == key) {
        return Some(Ignored::Fixed);
    }
    const BLOCKED_EXACT: [(&str, &str); 12] = [
        ("scripts", "runs scripts"),
        ("include", "loads another configuration file"),
        ("config-dir", "loads another configuration"),
        ("use-filedir-conf", "loads configuration next to the video"),
        ("log-file", "writes a file"),
        ("dump-stats", "writes a file"),
        ("stream-dump", "writes a file"),
        ("stream-record", "writes a file"),
        ("record-file", "writes a file"),
        ("write-filename-in-watch-later-config", "writes files"),
        ("profile-restore", "not supported in presets"),
        ("player-operation-mode", "changes the player's mode"),
    ];
    const BLOCKED_PREFIX: [(&str, &str); 6] = [
        ("script", "runs scripts"),
        ("load-", "loads scripts or other files"),
        ("input-", "input configuration, IPC sockets"),
        ("watch-later-", "writes files"),
        ("screenshot-", "Nikomiru takes the screenshots (PLY-011)"),
        ("ytdl", "runs youtube-dl / yt-dlp"),
    ];
    const NO_EFFECT_EXACT: [(&str, &str); 2] = [
        ("gpu-api", "no effect with the libmpv render API"),
        ("gpu-context", "no effect with the libmpv render API"),
    ];
    const NO_EFFECT_PREFIX: [(&str, &str); 2] = [
        ("vo-", "no effect with the libmpv render API"),
        ("sub-", "Nikomiru draws text subtitles itself"),
    ];
    if let Some((_, r)) = BLOCKED_EXACT.iter().find(|(k, _)| *k == key) {
        return Some(Ignored::Blocked(r));
    }
    if let Some((_, r)) = BLOCKED_PREFIX.iter().find(|(p, _)| key.starts_with(p)) {
        return Some(Ignored::Blocked(r));
    }
    if let Some((_, r)) = NO_EFFECT_EXACT.iter().find(|(k, _)| *k == key) {
        return Some(Ignored::NoEffect(r));
    }
    if let Some((_, r)) = NO_EFFECT_PREFIX.iter().find(|(p, _)| key.starts_with(p)) {
        return Some(Ignored::NoEffect(r));
    }
    None
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !key.starts_with('-')
}

/// Removes a trailing `# comment` that is outside quotes.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Parses raw options or a preset in `mpv.conf` syntax: `key=value`, `key` (flag
/// = yes), `no-key`, `--key=value`, quoted values, `#` comments, blank lines.
pub fn parse_mpv_conf(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let s = strip_comment(raw).trim();
        if s.is_empty() {
            continue;
        }
        if s.starts_with('[') {
            out.push(Line::Ignore {
                line,
                name: s.to_string(),
                reason: Ignored::Section,
            });
            continue;
        }
        let s = s.strip_prefix("--").unwrap_or(s);
        let (key, value) = match s.split_once('=') {
            Some((k, v)) => {
                let v = v.trim();
                let v = v
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                    .unwrap_or(v);
                (k.trim().to_string(), v.to_string())
            }
            None => match s.strip_prefix("no-") {
                Some(k) => (k.to_string(), "no".to_string()),
                None => (s.to_string(), "yes".to_string()),
            },
        };
        if !valid_key(&key) {
            out.push(Line::Invalid {
                line,
                error: format!("`{key}` is not a valid option name"),
            });
            continue;
        }
        if key == "profile" && !BUILTIN_PROFILES.contains(&value.as_str()) {
            out.push(Line::Invalid {
                line,
                error: format!("unknown profile `{value}`: only built-in profiles are supported"),
            });
            continue;
        }
        match classify(&key) {
            Some(reason) => out.push(Line::Ignore {
                line,
                name: key,
                reason,
            }),
            None => out.push(Line::Apply {
                line,
                option: MpvOption { key, value },
            }),
        }
    }
    out
}

/// Options set on the core before `mpv_initialize`, after the fixed ones. The
/// render context probes the hwdec interops when it is created, so the interop
/// cannot wait for the first `loadfile` (SPECIFICATION §5.2.2 #10): the CUDA
/// interop only with NVIDIA's proprietary driver, otherwise probing it prints
/// `Cannot load libcuda.so.1` on stderr.
pub fn core_options(nvidia_driver: bool) -> Vec<MpvOption> {
    if nvidia_driver {
        Vec::new()
    } else {
        vec![MpvOption::new("gpu-hwdec-interop", "vaapi")]
    }
}

/// The options applied before each `loadfile`, in order (PLY-006): the settings,
/// then the UI/preset options, then the raw options, which win. Returns the
/// options and the report of the raw lines.
pub fn load_options(cfg: &PlayerConfig) -> (Vec<MpvOption>, Vec<Line>) {
    let mut out = vec![
        MpvOption::new("hwdec", &cfg.hwdec),
        MpvOption::new("audio-pitch-correction", "yes"),
        MpvOption::new("tone-mapping", "auto"),
        MpvOption::new("audio-channels", "auto-safe"),
    ];
    // PLY-018: `fast` or `high-quality`; `default` needs nothing
    let profile = cfg.quality_profile.as_str();
    if profile != "default" && BUILTIN_PROFILES.contains(&profile) {
        out.push(MpvOption::new("profile", profile));
    }
    for (k, v) in &cfg.mpv_options {
        if classify(k).is_some() {
            continue;
        }
        out.push(MpvOption::new(k, v));
        if k == "interpolation" && v == "yes" {
            if !cfg.mpv_options.contains_key("video-sync") {
                out.push(MpvOption::new("video-sync", "display-resample"));
            }
            if !cfg.mpv_options.contains_key("tscale") {
                out.push(MpvOption::new("tscale", "oversample"));
            }
        }
    }
    let raw = parse_mpv_conf(&cfg.raw_mpv_options);
    out.extend(raw.iter().filter_map(|l| match l {
        Line::Apply { option, .. } => Some(option.clone()),
        _ => None,
    }));
    (out, raw)
}

/// Whether NVIDIA's proprietary driver is loaded (SPECIFICATION §5.2.2).
pub fn nvidia_driver_loaded() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(lines: &[Line]) -> Vec<(String, String)> {
        lines
            .iter()
            .filter_map(|l| match l {
                Line::Apply { option, .. } => Some((option.key.clone(), option.value.clone())),
                _ => None,
            })
            .collect()
    }

    // T-PLY-006
    #[test]
    fn raw_options_syntax() {
        let text = "\
# comment
deband=yes
--scale=ewa_lanczos   # trailing comment
interpolation
no-audio-display
glsl-shaders=\"~/a # b.glsl\"
key without equals
";
        let lines = parse_mpv_conf(text);
        assert_eq!(
            applied(&lines),
            [
                ("deband".into(), "yes".into()),
                ("scale".into(), "ewa_lanczos".into()),
                ("interpolation".into(), "yes".into()),
                ("audio-display".into(), "no".into()),
                ("glsl-shaders".into(), "~/a # b.glsl".into()),
            ]
        );
        assert!(
            matches!(lines.last(), Some(Line::Invalid { line: 7, .. })),
            "{lines:?}"
        );
    }

    // T-PLY-017
    #[test]
    fn blocked_and_ignored_options_never_apply() {
        let text = "\
vo=gpu
script=evil.lua
scripts=a.lua
load-scripts=yes
include=/tmp/x.conf
input-ipc-server=/tmp/s
log-file=/tmp/l
watch-later-dir=/tmp
screenshot-directory=/tmp
ytdl-format=best
gpu-api=vulkan
sub-font=Arial
[my-profile]
profile=my-profile
profile=high-quality
";
        let lines = parse_mpv_conf(text);
        assert_eq!(applied(&lines), [("profile".into(), "high-quality".into())]);
        let reasons: Vec<_> = lines
            .iter()
            .filter_map(|l| match l {
                Line::Ignore { reason, .. } => Some(reason.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(reasons[0], Ignored::Fixed);
        assert!(
            reasons[1..10]
                .iter()
                .all(|r| matches!(r, Ignored::Blocked(_))),
            "{reasons:?}"
        );
        assert!(matches!(reasons[10], Ignored::NoEffect(_)));
        assert!(matches!(reasons[11], Ignored::NoEffect(_)));
        assert_eq!(reasons[12], Ignored::Section);
        assert!(
            lines
                .iter()
                .any(|l| matches!(l, Line::Invalid { line: 14, .. }))
        );
    }

    #[test]
    fn every_fixed_option_is_rejected() {
        for (k, _) in FIXED {
            assert_eq!(classify(k), Some(Ignored::Fixed), "{k}");
        }
        assert_eq!(classify("hwdec"), None);
        assert_eq!(classify("deband"), None);
    }

    // T-PLY-006
    #[test]
    fn load_options_order_and_rules() {
        let cfg = PlayerConfig {
            quality_profile: "high-quality".into(),
            mpv_options: [("interpolation", "yes"), ("script", "x.lua")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            raw_mpv_options: "deband=yes\nhwdec=no\n".into(),
            ..PlayerConfig::default()
        };
        let (opts, report) = load_options(&cfg);
        let keys: Vec<&str> = opts.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "hwdec",
                "audio-pitch-correction",
                "tone-mapping",
                "audio-channels",
                "profile",
                "interpolation",
                "video-sync",
                "tscale",
                "deband",
                "hwdec",
            ]
        );
        // the raw options come last, so `hwdec=no` wins
        assert_eq!(opts.last(), Some(&MpvOption::new("hwdec", "no")));
        assert_eq!(report.len(), 2);
        // with NVIDIA's driver the CUDA interop stays available
        assert_eq!(
            core_options(false),
            [MpvOption::new("gpu-hwdec-interop", "vaapi")]
        );
        assert!(core_options(true).is_empty());
    }
}
