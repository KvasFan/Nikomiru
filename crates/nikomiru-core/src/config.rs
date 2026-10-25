// SPDX-License-Identifier: GPL-3.0-only

//! `config.toml` (SPECIFICATION §5.4, §9). Unknown keys are kept as they are and
//! written back, so a file from a newer version or edited by hand loses nothing.
//! Writes are atomic and the file is readable only by the user (0600).

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Errors while reading or writing the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file exists but could not be read.
    #[error("reading {path}: {source}")]
    Read {
        /// The configuration file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("writing {path}: {source}")]
    Write {
        /// The configuration file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML or has values of the wrong type. Only the
    /// parser's message and the line are kept: the parser's own text quotes the
    /// line, which may hold an API key (PRV-002).
    #[error("{path}, line {line}: {message}")]
    Parse {
        /// The configuration file.
        path: PathBuf,
        /// 1-based line of the error (0 if unknown).
        line: usize,
        /// The parser's message, without the quoted line.
        message: String,
    },
    /// The configuration could not be serialized (should not happen).
    #[error("serializing the configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `[general]`.
    pub general: General,
    /// `[player]`.
    pub player: PlayerConfig,
    /// `[subtitles]`.
    pub subtitles: SubtitlesConfig,
    /// `[lookup]`.
    pub lookup: LookupConfig,
    /// `[popup]`.
    pub popup: PopupConfig,
    /// `[anki]`.
    pub anki: AnkiConfig,
    /// `[paths]`.
    pub paths: PathsConfig,
    /// `[network]` (SET-007).
    pub network: NetworkConfig,
    /// `[online]` (ADR 0008).
    pub online: OnlineConfig,
    /// `[word_audio]` (AUD-001…AUD-008).
    pub word_audio: WordAudioConfig,
    /// `[shortcuts]`: action → keys, only for the bindings changed by the user
    /// (KEY-001). The defaults live in the UI.
    pub shortcuts: BTreeMap<String, Vec<String>>,
    /// Sections this version does not know, kept as they are.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[general]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// `"system"`, `"it"`, `"en"`, …
    pub ui_language: String,
    /// `"light"`, `"dark"` or `"system"`.
    pub theme: String,
    /// Language being studied (BCP 47), e.g. `"ja"`.
    pub study_language: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[player]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    /// mpv `hwdec` (PLY-006): `auto-safe`, `auto-copy-safe` or `no`.
    pub hwdec: String,
    /// `"default"` or `"high-quality"` (PLY-006).
    pub quality_profile: String,
    /// Seek step in seconds, 1–30 (PLY-001).
    pub seek_seconds: u8,
    /// Start the next episode at the end (LIB-009).
    pub auto_next: bool,
    /// Raw mpv options, `key=value` per line, applied last (PLY-006, PLY-017).
    pub raw_mpv_options: String,
    /// Last loaded or saved preset, `mpv/<name>.conf` (PLY-016).
    pub mpv_preset: String,
    /// mpv options set from the UI or loaded from a preset (PLY-016, PLY-018).
    pub mpv_options: BTreeMap<String, String>,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[subtitles]` (SUB-008, SUB-010, SUB-014).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitlesConfig {
    /// Languages of the subtitles chosen automatically, in order (SUB-008).
    pub preferred_languages: Vec<String>,
    /// Font size in pixels for a video 1080 px high (SUB-010).
    pub font_size: f64,
    /// Use the colours, fonts and positions of ASS files (SUB-010).
    pub respect_ass_styles: bool,
    /// `"none"`, `"blur"` or `"hidden"` (SUB-014).
    pub obscure: String,
    /// The same for the secondary subtitle (SUB-017).
    pub secondary_obscure: String,
    /// Where the secondary subtitle goes: `"top"` of the video, or `"bottom"`
    /// above the main one (SUB-017).
    pub secondary_position: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for SubtitlesConfig {
    fn default() -> Self {
        Self {
            preferred_languages: vec!["ja".into(), "jpn".into()],
            font_size: 36.0,
            respect_ass_styles: false,
            obscure: "none".into(),
            secondary_obscure: "none".into(),
            secondary_position: "top".into(),
            other: toml::Table::new(),
        }
    }
}

/// `[lookup]` (LKP-035, POP-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LookupConfig {
    /// Headword groups in a popup (LKP-035), 1–50.
    pub max_results: usize,
    /// Look words up on hover without holding Shift (POP-003).
    pub hover_without_modifier: bool,
    /// Close a popup opened on hover, and resume, when the pointer leaves
    /// the subtitles and the popup (POP-003).
    pub resume_on_leave: bool,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for LookupConfig {
    fn default() -> Self {
        Self {
            max_results: 10,
            hover_without_modifier: false,
            resume_on_leave: true,
            other: toml::Table::new(),
        }
    }
}

/// `[popup]` (POP-006): the largest dictionary popup, in points. A popup is
/// this wide and as tall as its content up to this height; the window can make
/// it smaller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PopupConfig {
    /// Width, within [`PopupConfig::WIDTHS`].
    pub max_width: f64,
    /// Height, within [`PopupConfig::HEIGHTS`].
    pub max_height: f64,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl PopupConfig {
    /// The widths a popup can have: narrower, a glossary line holds a few
    /// words; no screen is wider.
    pub const WIDTHS: RangeInclusive<f64> = 240.0..=4096.0;
    /// The heights a popup can have: lower, it shows less than a headword
    /// and its first line.
    pub const HEIGHTS: RangeInclusive<f64> = 120.0..=4096.0;

    /// A size brought within [`Self::WIDTHS`] and [`Self::HEIGHTS`]; a
    /// value that is not a number takes the default.
    pub fn bounded(width: f64, height: f64) -> (f64, f64) {
        let defaults = Self::default();
        let fit = |value: f64, range: &RangeInclusive<f64>, default: f64| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                default
            }
        };
        (
            fit(width, &Self::WIDTHS, defaults.max_width),
            fit(height, &Self::HEIGHTS, defaults.max_height),
        )
    }
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            max_width: 560.0,
            max_height: 500.0,
            other: toml::Table::new(),
        }
    }
}

/// `[anki]` (SPECIFICATION §9, ANK-001…ANK-036).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnkiConfig {
    /// AnkiConnect's address (ANK-001).
    pub url: String,
    /// AnkiConnect's API key; empty: none. Never logged (PRV-002).
    pub api_key: String,
    /// The deck cards go to.
    pub deck: String,
    /// The note type of the cards.
    pub note_type: String,
    /// `"deck"` or `"collection"` (ANK-030).
    pub duplicate_scope: String,
    /// Audio before the cue, 0–1000 ms (ANK-032).
    pub audio_pad_head_ms: u32,
    /// Audio after the cue, 0–1000 ms (ANK-032).
    pub audio_pad_tail_ms: u32,
    /// `"aac-m4a"`, `"opus-ogg"` or `"mp3"` (ANK-033).
    pub audio_format: String,
    /// 0 = mono 64k, 1 = stereo 128k, 2 = stereo 192k (ANK-033).
    pub audio_quality_level: u8,
    /// `"jpeg"` or `"webp"` (ANK-034).
    pub image_format: String,
    /// 0–3: long side 720, 1000, 2000 px or the original (ANK-034).
    pub image_quality_level: u8,
    /// The user's tags (ANK-027).
    pub tags: Vec<String>,
    /// Note type → field → template (ANK-020). A Lapis-like note type without
    /// a mapping gets the defaults of ANK-012.
    pub field_mapping: BTreeMap<String, BTreeMap<String, String>>,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl AnkiConfig {
    /// The audio pads' upper limit (ANK-032).
    pub const MAX_PAD_MS: u32 = 1000;

    fn bounded(mut self) -> Self {
        let defaults = Self::default();
        self.audio_pad_head_ms = self.audio_pad_head_ms.min(Self::MAX_PAD_MS);
        self.audio_pad_tail_ms = self.audio_pad_tail_ms.min(Self::MAX_PAD_MS);
        self.audio_quality_level = self.audio_quality_level.min(2);
        self.image_quality_level = self.image_quality_level.min(3);
        if !matches!(self.duplicate_scope.as_str(), "deck" | "collection") {
            self.duplicate_scope = defaults.duplicate_scope;
        }
        if !matches!(self.audio_format.as_str(), "aac-m4a" | "opus-ogg" | "mp3") {
            self.audio_format = defaults.audio_format;
        }
        if !matches!(self.image_format.as_str(), "jpeg" | "webp") {
            self.image_format = defaults.image_format;
        }
        self
    }
}

/// `[network]` (SET-007).
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkConfig {
    /// The proxy of the online services (`http://host:port`, `socks5://…`);
    /// empty: none. The environment's proxy variables are never used.
    pub proxy: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl std::fmt::Debug for NetworkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConfig")
            .field("proxy", &without_userinfo(&self.proxy))
            .finish_non_exhaustive()
    }
}

/// Jimaku's address (MET-003).
pub const JIMAKU_URL: &str = "https://jimaku.cc/api";

/// OpenSubtitles' address (MET-004).
pub const OPENSUBTITLES_URL: &str = "https://api.opensubtitles.com/api/v1";

fn is_jimaku_url(url: &str) -> bool {
    url == JIMAKU_URL
}

fn is_opensubtitles_url(url: &str) -> bool {
    url == OPENSUBTITLES_URL
}

/// `[word_audio]` (AUD-001…AUD-008).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WordAudioConfig {
    /// Play the first entry's audio when a popup opens (AUD-006).
    pub autoplay: bool,
    /// The volume of the word audio, 0–100 (AUD-005).
    pub volume: u8,
    /// The sources, in the order they are asked (AUD-001); none by default:
    /// no third-party address is built in (AUD-007).
    pub sources: Vec<AudioSourceConfig>,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for WordAudioConfig {
    fn default() -> Self {
        Self {
            autoplay: true,
            volume: 100,
            sources: Vec::new(),
            other: toml::Table::new(),
        }
    }
}

/// A word audio source: a URL template (AUD-002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSourceConfig {
    /// Its name, shown in the menu.
    pub name: String,
    /// The address, with `{term}` and `{reading}`.
    pub url: String,
    /// Asked, or not.
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// `[online]` (ADR 0008, MET-005).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OnlineConfig {
    /// The subtitle services used, in the order preferred (MET-011):
    /// `jimaku`, `opensubtitles`; one left out is not used.
    pub providers: Vec<String>,
    /// Search and load subtitles when a video has none in the preferred
    /// language (MET-007).
    pub auto_subtitles: bool,
    /// Jimaku's API key.
    pub jimaku_api_key: String,
    /// OpenSubtitles' consumer API key.
    pub opensubtitles_api_key: String,
    /// OpenSubtitles user name, for the download quota.
    pub opensubtitles_username: String,
    /// OpenSubtitles password.
    pub opensubtitles_password: String,
    /// Jimaku's address, another one for the tests.
    #[serde(skip_serializing_if = "is_jimaku_url")]
    pub jimaku_url: String,
    /// OpenSubtitles' address, another one for the tests.
    #[serde(skip_serializing_if = "is_opensubtitles_url")]
    pub opensubtitles_url: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for OnlineConfig {
    fn default() -> Self {
        Self {
            providers: vec!["jimaku".into(), "opensubtitles".into()],
            auto_subtitles: false,
            jimaku_api_key: String::new(),
            opensubtitles_api_key: String::new(),
            opensubtitles_username: String::new(),
            opensubtitles_password: String::new(),
            jimaku_url: JIMAKU_URL.into(),
            opensubtitles_url: OPENSUBTITLES_URL.into(),
            other: toml::Table::new(),
        }
    }
}

impl std::fmt::Debug for OnlineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // never a key or the password (MET-005, PRV-002)
        let set = |s: &str| if s.is_empty() { "" } else { "…" };
        f.debug_struct("OnlineConfig")
            .field("providers", &self.providers)
            .field("auto_subtitles", &self.auto_subtitles)
            .field("jimaku_api_key", &set(&self.jimaku_api_key))
            .field("opensubtitles_api_key", &set(&self.opensubtitles_api_key))
            .field("opensubtitles_username", &self.opensubtitles_username)
            .field("opensubtitles_password", &set(&self.opensubtitles_password))
            .finish_non_exhaustive()
    }
}

/// `url` without the user name and password it may hold, for messages and
/// logs (PRV-002): all before its last `@` after the scheme goes, whether it
/// parses or not.
pub fn without_userinfo(url: &str) -> String {
    let authority = url.find("://").map_or(0, |i| i + 3);
    match url[authority..].rfind('@') {
        Some(at) => format!("{}{}", &url[..authority], &url[authority + at + 1..]),
        None => url.to_string(),
    }
}

impl std::fmt::Debug for AnkiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // never the API key or a password in the address (PRV-002)
        f.debug_struct("AnkiConfig")
            .field("url", &without_userinfo(&self.url))
            .field("api_key", &if self.api_key.is_empty() { "" } else { "…" })
            .field("deck", &self.deck)
            .field("note_type", &self.note_type)
            .field("duplicate_scope", &self.duplicate_scope)
            .finish_non_exhaustive()
    }
}

impl Default for AnkiConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:8765".into(),
            api_key: String::new(),
            deck: "Lapis".into(),
            note_type: "Lapis".into(),
            duplicate_scope: "deck".into(),
            audio_pad_head_ms: 120,
            audio_pad_tail_ms: 200,
            audio_format: "aac-m4a".into(),
            audio_quality_level: 0,
            image_format: "jpeg".into(),
            image_quality_level: 1,
            tags: Vec::new(),
            field_mapping: BTreeMap::new(),
            other: toml::Table::new(),
        }
    }
}

/// `[paths]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    /// The ffmpeg executable, or its folder; empty = `NIKOMIRU_FFMPEG`, then
    /// `PATH` (EXT-001). ffprobe is looked for next to it.
    pub ffmpeg: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl Default for General {
    fn default() -> Self {
        Self {
            ui_language: "system".into(),
            theme: "system".into(),
            study_language: "ja".into(),
            other: toml::Table::new(),
        }
    }
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            hwdec: "auto-safe".into(),
            quality_profile: "default".into(),
            seek_seconds: 3,
            auto_next: true,
            raw_mpv_options: String::new(),
            mpv_preset: String::new(),
            mpv_options: BTreeMap::new(),
            other: toml::Table::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            player: PlayerConfig::default(),
            subtitles: SubtitlesConfig::default(),
            lookup: LookupConfig::default(),
            popup: PopupConfig::default(),
            anki: AnkiConfig::default(),
            paths: PathsConfig::default(),
            network: NetworkConfig::default(),
            online: OnlineConfig::default(),
            word_audio: WordAudioConfig::default(),
            shortcuts: BTreeMap::new(),
            other: toml::Table::new(),
        }
    }
}

impl Config {
    /// Parses a configuration and brings out-of-range values back into range.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        let mut c: Config = toml::from_str(text)?;
        c.player.seek_seconds = c.player.seek_seconds.clamp(1, 30);
        c.lookup.max_results = c.lookup.max_results.clamp(1, 50);
        if !(8.0..=200.0).contains(&c.subtitles.font_size) {
            c.subtitles.font_size = 36.0;
        }
        (c.popup.max_width, c.popup.max_height) =
            PopupConfig::bounded(c.popup.max_width, c.popup.max_height);
        c.anki = c.anki.bounded();
        Ok(c)
    }

    /// Reads `path`; a missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).map_err(|e| ConfigError::Parse {
                path: path.to_path_buf(),
                line: e.span().map_or(0, |s| {
                    text[..s.start.min(text.len())].lines().count().max(1)
                }),
                message: mask_quoted(e.message().trim()),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes `path` atomically with permissions 0600.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        crate::fs::write_atomic(path, text.as_bytes(), 0o600).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Dotted names of the keys this version does not know (for a warning).
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = self.other.keys().cloned().collect();
        out.extend(self.general.other.keys().map(|k| format!("general.{k}")));
        out.extend(self.player.other.keys().map(|k| format!("player.{k}")));
        out.extend(self.lookup.other.keys().map(|k| format!("lookup.{k}")));
        out.extend(self.popup.other.keys().map(|k| format!("popup.{k}")));
        out.extend(self.anki.other.keys().map(|k| format!("anki.{k}")));
        out.extend(self.paths.other.keys().map(|k| format!("paths.{k}")));
        out.extend(
            self.subtitles
                .other
                .keys()
                .map(|k| format!("subtitles.{k}")),
        );
        out.sort();
        out
    }
}

impl crate::Localized for ConfigError {
    fn message_id(&self) -> &'static str {
        match self {
            ConfigError::Read { .. } => "error-config-read",
            ConfigError::Write { .. } => "error-config-write",
            ConfigError::Parse { .. } => "error-config",
            ConfigError::Serialize(_) => "error-config-serialize",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            ConfigError::Read { path, source } | ConfigError::Write { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            ConfigError::Parse {
                path,
                line,
                message,
            } => vec![
                ("path", path.display().to_string()),
                ("line", line.to_string()),
                ("reason", message.clone()),
            ],
            ConfigError::Serialize(e) => vec![("reason", e.to_string())],
        }
    }
}

/// Replaces what serde quotes in its messages (`string "value"`, `` `value` ``)
/// with `…`: the value may be a secret typed into the wrong field.
fn mask_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut open: Option<char> = None;
    for c in message.chars() {
        match open {
            Some(q) if c == q => {
                out.push('…');
                out.push(c);
                open = None;
            }
            Some(_) => {}
            None => {
                out.push(c);
                if c == '"' || c == '`' {
                    open = Some(c);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.player.hwdec, "auto-safe");
        assert_eq!(c.player.seek_seconds, 3);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let text = r#"
future_section = { a = 1 }

[general]
theme = "dark"
new_flag = true

[player]
seek_seconds = 5
mpv_options = { deband = "yes" }
later = "x"

[anki]
deck = "Mining"
someday = 1

[shortcuts]
play_pause = ["Space"]
"#;
        let c = Config::from_toml(text).unwrap();
        assert_eq!(c.general.theme, "dark");
        assert_eq!(c.player.seek_seconds, 5);
        assert_eq!(c.player.mpv_options["deband"], "yes");
        assert_eq!(c.shortcuts["play_pause"], ["Space"]);
        assert_eq!(
            c.unknown_keys(),
            [
                "anki.someday",
                "future_section",
                "general.new_flag",
                "player.later"
            ]
        );
        let again = Config::from_toml(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn anki_settings_have_defaults_and_limits() {
        let c = Config::from_toml(
            r#"
[anki]
api_key = "s3cret"
audio_pad_head_ms = 5000
audio_quality_level = 9
image_quality_level = 7
duplicate_scope = "galaxy"
audio_format = "flac"
tags = ["mine"]

[anki.field_mapping.Lapis]
Expression = "{expression}"
"#,
        )
        .unwrap();
        let a = &c.anki;
        assert_eq!(a.audio_pad_head_ms, 1000);
        assert_eq!(a.audio_pad_tail_ms, 200);
        assert_eq!((a.audio_quality_level, a.image_quality_level), (2, 3));
        assert_eq!(
            (a.duplicate_scope.as_str(), a.audio_format.as_str()),
            ("deck", "aac-m4a")
        );
        assert_eq!(a.field_mapping["Lapis"]["Expression"], "{expression}");
        assert_eq!(a.tags, ["mine"]);
        // T-PRV-002: the key never shows in a debug print
        assert!(!format!("{c:?}").contains("s3cret"));
        let d = AnkiConfig::default();
        assert_eq!(
            (d.url.as_str(), d.deck.as_str()),
            ("http://127.0.0.1:8765", "Lapis")
        );
    }

    // T-MET-005
    #[test]
    fn online_settings_keep_their_secrets() {
        let c = Config::from_toml(
            r#"
[network]
proxy = "http://me:pw0@proxy.lan:3128"

[online]
auto_subtitles = true
jimaku_api_key = "jk-s3cret"
opensubtitles_api_key = "os-s3cret"
opensubtitles_username = "me"
opensubtitles_password = "pw-s3cret"
"#,
        )
        .unwrap();
        assert!(c.online.auto_subtitles);
        assert_eq!(c.online.jimaku_url, JIMAKU_URL);
        assert_eq!(c.online.opensubtitles_url, OPENSUBTITLES_URL);
        let shown = format!("{c:?}");
        for secret in ["jk-s3cret", "os-s3cret", "pw-s3cret", "pw0"] {
            assert!(!shown.contains(secret), "{secret}");
        }
        // the addresses are written only when they are not the services'
        let text = toml::to_string_pretty(&c).unwrap();
        assert!(!text.contains("jimaku_url"), "{text}");
        let mut test = c.clone();
        test.online.jimaku_url = "http://127.0.0.1:9/api".into();
        let text = toml::to_string_pretty(&test).unwrap();
        assert!(text.contains("jimaku_url"), "{text}");
        assert_eq!(Config::from_toml(&text).unwrap(), test);
        let d = Config::default();
        assert!(!d.online.auto_subtitles);
        assert!(d.network.proxy.is_empty());
    }

    #[test]
    fn lookup_settings_have_defaults_and_limits() {
        let c = Config::from_toml("[lookup]\nmax_results = 500\nhover_without_modifier = true")
            .unwrap();
        assert_eq!(c.lookup.max_results, 50);
        assert!(c.lookup.hover_without_modifier && c.lookup.resume_on_leave);
        assert_eq!(Config::default().lookup.max_results, 10);
    }

    // T-POP-006
    #[test]
    fn popup_sizes_have_defaults_and_bounds() {
        let size = |text: &str| {
            let c = Config::from_toml(text).unwrap();
            (c.popup.max_width, c.popup.max_height)
        };
        assert_eq!(size(""), (560.0, 500.0));
        // whole numbers, as people write them
        assert_eq!(
            size("[popup]\nmax_width = 700\nmax_height = 320"),
            (700.0, 320.0)
        );
        assert_eq!(
            size("[popup]\nmax_width = 10\nmax_height = 100000.5"),
            (240.0, 4096.0)
        );
        // TOML has floats that are not numbers
        assert_eq!(
            size("[popup]\nmax_width = nan\nmax_height = -inf"),
            (560.0, 500.0)
        );
        assert_eq!(PopupConfig::bounded(f64::INFINITY, 0.0), (560.0, 120.0));
        let c =
            Config::from_toml("[popup]\nmax_width = 600\nlater = 1\n[lookup]\nlater = 2").unwrap();
        assert_eq!(c.unknown_keys(), ["lookup.later", "popup.later"]);
        let again = Config::from_toml(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn seek_step_is_clamped() {
        assert_eq!(
            Config::from_toml("[player]\nseek_seconds = 0")
                .unwrap()
                .player
                .seek_seconds,
            1
        );
        assert_eq!(
            Config::from_toml("[player]\nseek_seconds = 99")
                .unwrap()
                .player
                .seek_seconds,
            30
        );
    }

    #[test]
    fn invalid_file_is_a_parse_error_with_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[anki]\napi_key = \"secret\"\n[player]\nseek_seconds = \"three\"\n",
        )
        .unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { line: 4, .. }), "{err}");
        let text = err.to_string();
        assert!(text.contains("config.toml"), "{text}");
        assert!(
            !text.contains("secret") && !text.contains("three"),
            "{text}"
        );
    }

    // T-SEC-008
    #[test]
    fn saved_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nikomiru/config.toml");
        Config::default().save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use serde::ser::Error as _;
        let io = || std::io::Error::other("x");
        let path = || PathBuf::from("/c/config.toml");
        let errors = [
            ConfigError::Read {
                path: path(),
                source: io(),
            },
            ConfigError::Write {
                path: path(),
                source: io(),
            },
            ConfigError::Parse {
                path: path(),
                line: 3,
                message: "x".into(),
            },
            ConfigError::Serialize(toml::ser::Error::custom("x")),
        ];
        let mut all: Vec<&dyn crate::Localized> = errors.iter().map(|e| e as _).collect();
        all.push(&crate::PathsError::NoHome);
        let problems = crate::error::message_problems(crate::error::I18N_DIR.as_ref(), &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
