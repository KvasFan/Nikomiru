// SPDX-License-Identifier: GPL-3.0-only

//! `config.toml` (SPECIFICATION §5.4, §9). Unknown keys are kept as they are and
//! written back, so a file from a newer version or edited by hand loses nothing;
//! a value of the wrong kind takes its default and is reported (SET-001).
//! Saving changes only the values that changed: the comments, the order and
//! the layout of a file edited by hand stay. Writes are atomic and the file
//! is readable only by the user (0600).

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
    /// `[sync]`: WebDAV (BAK-004).
    pub sync: SyncConfig,
    /// `[[servers]]`: the media servers added (SRV-001).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<ServerConfig>,
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
    /// `"light"`, `"dark"` or `"system"` (SET-003).
    pub theme: String,
    /// The accent color, `#rrggbb` (SET-003).
    pub accent: String,
    /// The interface scale in percent (SET-005), on top of the display's.
    pub ui_scale: u16,
    /// Language being studied (BCP 47), e.g. `"ja"`.
    pub study_language: String,
    /// The library shows its first steps until they are done or hidden
    /// (LIB-013).
    pub first_steps: bool,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl General {
    /// The interface scales offered, in percent (SET-005).
    pub const SCALES: RangeInclusive<u16> = 75..=200;
    /// The default accent.
    pub const ACCENT: &str = "#6aa0ff";

    /// The accent as red, green and blue; `None` if it is not `#rrggbb`.
    pub fn accent_rgb(&self) -> Option<[u8; 3]> {
        parse_rgb(&self.accent)
    }
}

/// `#rrggbb` as red, green and blue.
pub fn parse_rgb(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A tab of the settings page whose keys can go back to their defaults
/// together (SET-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Language, theme, accent, scale; the seek step and the next episode.
    General,
    /// `[subtitles]`.
    Subtitles,
    /// `[lookup]` and `[popup]`.
    Lookup,
    /// The word audio's playing; its sources stay.
    WordAudio,
    /// `[anki]` but the key of AnkiConnect.
    Anki,
    /// The subtitle services and the proxy; their keys and login stay.
    Services,
    /// `[shortcuts]`.
    Shortcuts,
    /// `[paths]`.
    Advanced,
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

/// `[subtitles]` (SUB-008, SUB-010, SUB-014, LKP-051, LKP-061).
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
    /// Readings over the kanji of the subtitles, from the dictionaries
    /// (LKP-051).
    pub furigana: bool,
    /// Underline the words already made into cards (LKP-061).
    pub mark_mined: bool,
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
            furigana: false,
            mark_mined: true,
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
    /// `"jpeg"` or `"webp"` (ANK-034); `"webp-animated"` or
    /// `"avif-animated"`: the sentence as an animated picture (ANK-042).
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
        if !matches!(
            self.image_format.as_str(),
            "jpeg" | "webp" | "webp-animated" | "avif-animated"
        ) {
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
    /// This installation's id for media servers (SRV-002, SRV-003): made when
    /// the first one is added.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub device_id: String,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `[sync]`: the WebDAV folder (BAK-004).
#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// The folder's address; empty: no sync.
    pub url: String,
    /// The user.
    pub user: String,
    /// The password (MET-005).
    pub password: String,
    /// Plain HTTP allowed outside the user's own networks (SRV-005).
    pub allow_unencrypted: bool,
    /// Sync at start and close (BAK-009).
    pub automatic: bool,
    /// Keys this version does not know.
    #[serde(flatten)]
    pub other: toml::Table,
}

impl std::fmt::Debug for SyncConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // never the password (MET-005, PRV-002)
        f.debug_struct("SyncConfig")
            .field("url", &without_userinfo(&self.url))
            .field("user", &self.user)
            .field("automatic", &self.automatic)
            .finish_non_exhaustive()
    }
}

/// A media server (SRV-001).
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Random, for the uids of its videos (SRV-007).
    pub id: String,
    /// `jellyfin` or `plex`.
    pub kind: String,
    /// The name shown.
    pub name: String,
    /// Its address; for Plex, the ones the account gave, the one to try first
    /// in front.
    pub url: String,
    /// The other addresses of a Plex server.
    pub other_urls: Vec<String>,
    /// Its token (MET-005).
    pub token: String,
    /// The user's id on Jellyfin.
    pub user_id: String,
    /// The libraries shown; empty: all (SRV-006).
    pub libraries: Vec<String>,
    /// Plain HTTP allowed outside the user's own networks (SRV-005).
    pub allow_unencrypted: bool,
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // never the token (SRV-004)
        f.debug_struct("ServerConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("name", &self.name)
            .field("url", &without_userinfo(&self.url))
            .field("libraries", &self.libraries)
            .field("allow_unencrypted", &self.allow_unencrypted)
            .finish_non_exhaustive()
    }
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

/// AniList's address (MET-001).
pub const ANILIST_URL: &str = "https://graphql.anilist.co";

/// Jikan's address (MET-002).
pub const JIKAN_URL: &str = "https://api.jikan.moe/v4";
/// The real plex.tv (SRV-003).
pub const PLEX_TV_URL: &str = "https://plex.tv/";

fn is_plex_tv_url(url: &str) -> bool {
    url == PLEX_TV_URL
}

fn is_anilist_url(url: &str) -> bool {
    url == ANILIST_URL
}

fn is_jikan_url(url: &str) -> bool {
    url == JIKAN_URL
}

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

/// A word audio source: a URL template (AUD-002), or the path of a
/// database of the Local Audio Server (AUD-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSourceConfig {
    /// Its name, shown in the menu.
    pub name: String,
    /// The address, with `{term}` and `{reading}`; or the database's path
    /// (`/…` or `file://…`).
    pub url: String,
    /// Asked, or not.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// For a database, its sub-sources in the order preferred (AUD-003);
    /// empty: `nhk16`, `forvo`, `jpod`, then the others.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub priority: Vec<String>,
}

impl AudioSourceConfig {
    /// The database's path, when the source is one (AUD-003).
    pub fn database(&self) -> Option<&str> {
        let url = self.url.trim();
        url.strip_prefix("file://")
            .or_else(|| url.starts_with('/').then_some(url))
    }
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
    /// Where the metadata of a series come from (MET-001, MET-002):
    /// `anilist` or `jikan`.
    pub metadata: String,
    /// AniList's address, another one for the tests.
    #[serde(skip_serializing_if = "is_anilist_url")]
    pub anilist_url: String,
    /// Jikan's address, another one for the tests.
    #[serde(skip_serializing_if = "is_jikan_url")]
    pub jikan_url: String,
    /// Another plex.tv, for the tests (SRV-003).
    #[serde(skip_serializing_if = "is_plex_tv_url")]
    pub plex_tv_url: String,
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
            metadata: "anilist".into(),
            anilist_url: ANILIST_URL.into(),
            jikan_url: JIKAN_URL.into(),
            plex_tv_url: PLEX_TV_URL.into(),
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
            .field("metadata", &self.metadata)
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
            accent: Self::ACCENT.into(),
            ui_scale: 100,
            first_steps: true,
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
            sync: SyncConfig::default(),
            servers: Vec::new(),
            other: toml::Table::new(),
        }
    }
}

impl Config {
    /// Parses a configuration and brings out-of-range values back into range;
    /// a value of the wrong kind is an error.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        Ok(toml::from_str::<Config>(text)?.bounded())
    }

    /// Parses a configuration; a value of the wrong kind takes its default
    /// and its dotted name is returned (SET-001). Only a file that is not
    /// TOML is an error.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), toml::de::Error> {
        let mut table: toml::Table = toml::from_str(text)?;
        let mut invalid = Vec::new();
        let fits = |section: &str, value: toml::Value| {
            let mut t = toml::Table::new();
            t.insert(section.to_string(), value);
            Config::deserialize(t).is_ok()
        };
        let sections: Vec<String> = table.keys().cloned().collect();
        for section in sections {
            let Some(value) = table.get_mut(&section) else {
                continue;
            };
            if fits(&section, value.clone()) {
                continue;
            }
            // a section of known keys: only the keys that do not fit go
            if let toml::Value::Table(keys) = value {
                let names: Vec<String> = keys.keys().cloned().collect();
                for name in names {
                    let mut one = toml::Table::new();
                    if let Some(v) = keys.get(&name) {
                        one.insert(name.clone(), v.clone());
                    }
                    if !fits(&section, toml::Value::Table(one)) {
                        keys.remove(&name);
                        invalid.push(format!("{section}.{name}"));
                    }
                }
                if fits(&section, value.clone()) {
                    continue;
                }
            }
            table.remove(&section);
            invalid.push(section);
        }
        let config = Config::deserialize(table)?.bounded();
        Ok((config, invalid))
    }

    /// Out-of-range values brought back into range.
    fn bounded(self) -> Self {
        let mut c = self;
        c.general.ui_scale = c
            .general
            .ui_scale
            .clamp(*General::SCALES.start(), *General::SCALES.end());
        if c.general.accent_rgb().is_none() {
            c.general.accent = General::ACCENT.into();
        }
        c.player.seek_seconds = c.player.seek_seconds.clamp(1, 30);
        c.lookup.max_results = c.lookup.max_results.clamp(1, 50);
        if !(8.0..=200.0).contains(&c.subtitles.font_size) {
            c.subtitles.font_size = 36.0;
        }
        (c.popup.max_width, c.popup.max_height) =
            PopupConfig::bounded(c.popup.max_width, c.popup.max_height);
        c.anki = c.anki.bounded();
        c
    }

    /// Reads `path`, with the dotted names of the values that took their
    /// default ([`Self::parse`]); a missing file gives the defaults.
    pub fn load(path: &Path) -> Result<(Self, Vec<String>), ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| ConfigError::Parse {
                path: path.to_path_buf(),
                line: e.span().map_or(0, |s| {
                    text[..s.start.min(text.len())].lines().count().max(1)
                }),
                message: mask_quoted(e.message().trim()),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Self::default(), Vec::new())),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes `path` atomically with permissions 0600. What is there already
    /// keeps its comments and layout: only the values that changed are
    /// written over, the keys gone are removed, the new ones added.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let fresh = toml::to_string_pretty(self)?;
        let old = std::fs::read_to_string(path).ok();
        let text = match old.as_deref().map(str::parse::<toml_edit::DocumentMut>) {
            Some(Ok(mut doc)) => match fresh.parse::<toml_edit::DocumentMut>() {
                Ok(new) => {
                    merge(doc.as_table_mut(), new.as_table());
                    doc.to_string()
                }
                Err(_) => fresh,
            },
            // nothing there, or not TOML: the settings as they are
            _ => fresh,
        };
        crate::fs::write_atomic(path, text.as_bytes(), 0o600).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// These settings with the TOML `text` laid over them, table by table:
    /// a key `text` has takes its value, the others stay.
    pub fn laid_over(&self, text: &str) -> Result<Self, String> {
        fn lay(base: &mut toml::Table, top: toml::Table) {
            for (key, value) in top {
                match (base.get_mut(&key), value) {
                    (Some(toml::Value::Table(b)), toml::Value::Table(t)) => lay(b, t),
                    (_, value) => {
                        base.insert(key, value);
                    }
                }
            }
        }
        let mut base = toml::Table::try_from(self).map_err(|e| e.to_string())?;
        let top: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        lay(&mut base, top);
        let text = toml::to_string(&base).map_err(|e| e.to_string())?;
        Self::from_toml(&text).map_err(|e| e.to_string())
    }

    /// Takes the keys of `section` from `from`, the others staying: what a
    /// tab of the settings saves.
    pub fn copy_section(&mut self, section: Section, from: &Config) {
        match section {
            Section::General => {
                self.general = from.general.clone();
                self.player.seek_seconds = from.player.seek_seconds;
                self.player.auto_next = from.player.auto_next;
            }
            Section::Subtitles => self.subtitles = from.subtitles.clone(),
            Section::Lookup => {
                self.lookup = from.lookup.clone();
                self.popup = from.popup.clone();
            }
            Section::WordAudio => self.word_audio = from.word_audio.clone(),
            Section::Anki => self.anki = from.anki.clone(),
            Section::Services => {
                self.online = from.online.clone();
                self.network = from.network.clone();
            }
            Section::Shortcuts => self.shortcuts = from.shortcuts.clone(),
            Section::Advanced => self.paths = from.paths.clone(),
        }
    }

    /// Puts the keys of `section` back to their defaults (SET-006). The
    /// other keys stay, and so do the keys this version does not know, the
    /// keys and passwords of the services, and the word audio's sources:
    /// what the user has, not how Nikomiru behaves.
    pub fn reset(&mut self, section: Section) {
        let d = Config::default();
        match section {
            Section::General => {
                let other = std::mem::take(&mut self.general.other);
                let study = std::mem::take(&mut self.general.study_language);
                self.general = General {
                    study_language: study,
                    other,
                    ..d.general
                };
                self.player.seek_seconds = d.player.seek_seconds;
                self.player.auto_next = d.player.auto_next;
            }
            Section::Subtitles => {
                let other = std::mem::take(&mut self.subtitles.other);
                self.subtitles = SubtitlesConfig {
                    other,
                    ..d.subtitles
                };
            }
            Section::Lookup => {
                let other = std::mem::take(&mut self.lookup.other);
                self.lookup = LookupConfig { other, ..d.lookup };
                let other = std::mem::take(&mut self.popup.other);
                self.popup = PopupConfig { other, ..d.popup };
            }
            Section::WordAudio => {
                self.word_audio.autoplay = d.word_audio.autoplay;
                self.word_audio.volume = d.word_audio.volume;
            }
            Section::Anki => {
                let other = std::mem::take(&mut self.anki.other);
                let api_key = std::mem::take(&mut self.anki.api_key);
                self.anki = AnkiConfig {
                    api_key,
                    other,
                    ..d.anki
                };
            }
            Section::Services => {
                self.online.metadata = d.online.metadata;
                self.online.providers = d.online.providers;
                self.online.auto_subtitles = d.online.auto_subtitles;
                self.network.proxy = d.network.proxy;
            }
            Section::Shortcuts => self.shortcuts.clear(),
            Section::Advanced => {
                let other = std::mem::take(&mut self.paths.other);
                self.paths = PathsConfig { other, ..d.paths };
            }
        }
    }

    /// These settings with every key, password and token blank, for a backup
    /// (BAK-002).
    pub fn without_secrets(&self) -> Self {
        let mut out = self.clone();
        out.anki.api_key.clear();
        out.online.jimaku_api_key.clear();
        out.online.opensubtitles_api_key.clear();
        out.online.opensubtitles_password.clear();
        out.sync.password.clear();
        for server in &mut out.servers {
            server.token.clear();
        }
        out
    }

    /// The keys blank here, taken from `current` (BAK-003).
    pub fn fill_secrets(&mut self, current: &Self) {
        let fill = |to: &mut String, from: &str| {
            if to.is_empty() {
                *to = from.to_string();
            }
        };
        fill(&mut self.anki.api_key, &current.anki.api_key);
        fill(
            &mut self.online.jimaku_api_key,
            &current.online.jimaku_api_key,
        );
        fill(
            &mut self.online.opensubtitles_api_key,
            &current.online.opensubtitles_api_key,
        );
        fill(
            &mut self.online.opensubtitles_password,
            &current.online.opensubtitles_password,
        );
        fill(&mut self.sync.password, &current.sync.password);
        for server in &mut self.servers {
            if let Some(same) = current.servers.iter().find(|s| s.id == server.id) {
                fill(&mut server.token, &same.token);
            }
        }
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
        out.extend(self.network.other.keys().map(|k| format!("network.{k}")));
        out.extend(self.online.other.keys().map(|k| format!("online.{k}")));
        out.extend(self.sync.other.keys().map(|k| format!("sync.{k}")));
        out.extend(
            self.word_audio
                .other
                .keys()
                .map(|k| format!("word_audio.{k}")),
        );
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

/// Writes `new` over `old`: a value that changed takes the new one with the
/// comments of the old, a table that changed is merged key by key, a key
/// gone is removed, a new one added at the end of its table.
fn merge(old: &mut dyn toml_edit::TableLike, new: &dyn toml_edit::TableLike) {
    let gone: Vec<String> = old
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| !new.contains_key(k))
        .collect();
    for key in gone {
        old.remove(&key);
    }
    for (key, item) in new.iter() {
        match old.get_mut(key) {
            Some(slot) => update(slot, item),
            None => {
                old.insert(key, item.clone());
            }
        }
    }
}

fn update(slot: &mut toml_edit::Item, new: &toml_edit::Item) {
    use toml_edit::Item;
    if same_item(slot, new) {
        return;
    }
    if let (Some(o), Some(n)) = (slot.as_inline_table_mut(), new.as_table_like()) {
        merge(o, n);
        // `{ a = 1, b = 2 }` less `b` is not `{ a = 1}`
        o.fmt();
        return;
    }
    if let (Some(o), Some(n)) = (slot.as_table_like_mut(), new.as_table_like()) {
        merge(o, n);
        return;
    }
    match (slot, new) {
        (Item::Value(o), Item::Value(n)) => {
            let decor = o.decor().clone();
            *o = n.clone();
            *o.decor_mut() = decor;
        }
        (Item::ArrayOfTables(o), Item::ArrayOfTables(n)) if o.len() == n.len() => {
            for (o, n) in o.iter_mut().zip(n.iter()) {
                merge(o, n);
            }
        }
        (slot, new) => *slot = new.clone(),
    }
}

/// Whether two items hold the same values, whatever their layout.
fn same_item(a: &toml_edit::Item, b: &toml_edit::Item) -> bool {
    use toml_edit::Item;
    match (a, b) {
        (Item::Value(a), Item::Value(b)) => same(a, b),
        (Item::ArrayOfTables(a), Item::ArrayOfTables(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| same_table(a, b))
        }
        _ => match (a.as_table_like(), b.as_table_like()) {
            (Some(a), Some(b)) => same_table(a, b),
            _ => false,
        },
    }
}

fn same_table(a: &dyn toml_edit::TableLike, b: &dyn toml_edit::TableLike) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| same_item(v, w)))
}

fn same(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    use toml_edit::Value;
    match (a, b) {
        (Value::String(a), Value::String(b)) => a.value() == b.value(),
        (Value::Integer(a), Value::Integer(b)) => a.value() == b.value(),
        (Value::Float(a), Value::Float(b)) => a.value() == b.value(),
        // `font_size = 36` is the float the settings write as `36.0`
        #[allow(clippy::cast_precision_loss)]
        (Value::Integer(i), Value::Float(f)) | (Value::Float(f), Value::Integer(i)) => {
            *i.value() as f64 == *f.value()
        }
        (Value::Boolean(a), Value::Boolean(b)) => a.value() == b.value(),
        (Value::Datetime(a), Value::Datetime(b)) => a.value() == b.value(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| same(a, b))
        }
        (Value::InlineTable(a), Value::InlineTable(b)) => same_table(a, b),
        _ => false,
    }
}

/// Whether Nikomiru can write in the settings folder `dir`: a file made and
/// removed there. A folder not there yet is fine (saving makes it); one
/// owned by another user (made with `sudo`) is not.
pub fn check_writable(dir: &Path) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let probe = dir.join(".nikomiru-write-check");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    std::fs::remove_file(&probe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let (c, invalid) = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(c, Config::default());
        assert!(invalid.is_empty());
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
    // T-BAK-002, T-BAK-003
    fn a_backup_leaves_the_secrets_out_and_a_restore_puts_them_back() {
        let mut config = Config::default();
        config.anki.api_key = "s3cret-a".into();
        config.online.jimaku_api_key = "s3cret-b".into();
        config.online.opensubtitles_api_key = "s3cret-c".into();
        config.online.opensubtitles_password = "s3cret-d".into();
        config.sync.password = "s3cret-e".into();
        config.servers.push(ServerConfig {
            id: "a".into(),
            token: "s3cret-f".into(),
            ..ServerConfig::default()
        });
        let mut blank = config.without_secrets();
        let text = toml::to_string(&blank).unwrap();
        assert!(!text.contains("s3cret"), "{text}");
        blank.fill_secrets(&config);
        assert_eq!(blank, config);
    }

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
            "[anki]\napi_key = \"secret\"\n[player]\nseek_seconds = \"three\n",
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
        assert_eq!(Config::load(&path).unwrap().0, Config::default());
    }

    // T-SET-001
    #[test]
    fn a_wrong_value_takes_its_default_and_is_told() {
        let (c, invalid) = Config::parse(
            "lookup = 7\n\
             [general]\ntheme = \"light\"\nui_scale = \"big\"\naccent = \"blue\"\n\
             [player]\nseek_seconds = \"three\"\nauto_next = false\n\
             [network]\nproxy = 3\nlater = 1\n",
        )
        .unwrap();
        assert_eq!(
            invalid,
            [
                "general.ui_scale",
                "lookup",
                "network.proxy",
                "player.seek_seconds"
            ]
        );
        let d = Config::default();
        assert_eq!(c.general.theme, "light");
        assert_eq!(c.general.ui_scale, 100);
        // not a color: the default, as for a number out of range
        assert_eq!(c.general.accent, General::ACCENT);
        assert_eq!(c.player.seek_seconds, d.player.seek_seconds);
        assert!(!c.player.auto_next);
        assert_eq!(c.lookup, d.lookup);
        assert_eq!(c.network.proxy, "");
        assert_eq!(c.unknown_keys(), ["network.later"]);
        assert_eq!(
            Config::parse("[general]\nui_scale = 500")
                .unwrap()
                .0
                .general
                .ui_scale,
            200
        );
        assert_eq!(
            Config::parse("[general]\naccent = \"#A0b1C2\"")
                .unwrap()
                .0
                .general
                .accent_rgb(),
            Some([0xa0, 0xb1, 0xc2])
        );
        assert!(Config::parse("[general\n").is_err());
    }

    // T-SET-001: what the user wrote stays
    #[test]
    fn saving_keeps_the_comments_and_the_layout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let text = "# my settings\n\n[player]\n# a bigger step\nseek_seconds = 5 # seconds\n\
                    mpv_options = { deband = \"yes\", scale = \"ewa_lanczos\" }\n\n\
                    [subtitles]\nfont_size = 40 # a little bigger\n\n\
                    [general]\ntheme = \"dark\"   # at night\nfuture = 1\n";
        std::fs::write(&path, text).unwrap();
        let (mut c, _) = Config::load(&path).unwrap();
        c.general.theme = "light".into();
        c.player.mpv_options.remove("scale");
        c.save(&path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(
            saved.starts_with(
                "# my settings\n\n[player]\n# a bigger step\nseek_seconds = 5 # seconds\n"
            ),
            "{saved}"
        );
        assert!(
            saved.contains("font_size = 40 # a little bigger\n"),
            "{saved}"
        );
        assert!(
            saved.contains("theme = \"light\"   # at night\n"),
            "{saved}"
        );
        assert!(
            saved.contains("mpv_options = { deband = \"yes\" }\n")
                && !saved.contains("ewa_lanczos"),
            "{saved}"
        );
        assert!(saved.contains("future = 1"), "{saved}");
        assert_eq!(Config::load(&path).unwrap(), (c.clone(), Vec::new()));
        // saved again unchanged: the same text
        c.save(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    }

    // T-SET-006
    #[test]
    fn a_section_goes_back_to_its_defaults_alone() {
        let (mut c, _) = Config::parse(
            "[general]\ntheme = \"light\"\nui_scale = 150\nlater = 1\n\
             [player]\nseek_seconds = 10\nhwdec = \"no\"\n\
             [subtitles]\nfont_size = 50\n\
             [lookup]\nmax_results = 3\n[popup]\nmax_width = 900\n\
             [online]\nproviders = [\"jimaku\"]\njimaku_api_key = \"k\"\n\
             [network]\nproxy = \"http://p:1\"\n\
             [anki]\ndeck = \"Mining\"\napi_key = \"a\"\n\
             [paths]\nffmpeg = \"/opt/ffmpeg\"\n\
             [shortcuts]\nplay_pause = [\"K\"]\n\
             [word_audio]\nvolume = 50\n\
             [[word_audio.sources]]\nname = \"Local\"\nurl = \"http://localhost:5050/?term={term}\"\n",
        )
        .unwrap();
        let d = Config::default();
        let before = c.clone();
        c.reset(Section::General);
        assert_eq!(
            (
                c.general.theme.as_str(),
                c.general.ui_scale,
                c.player.seek_seconds
            ),
            ("system", 100, 3)
        );
        assert_eq!(c.general.other, before.general.other);
        assert_eq!(c.player.hwdec, "no");
        assert_eq!(
            (&c.subtitles, &c.lookup, &c.anki),
            (&before.subtitles, &before.lookup, &before.anki)
        );
        c.reset(Section::Subtitles);
        assert_eq!(c.subtitles, d.subtitles);
        c.reset(Section::Lookup);
        assert_eq!((&c.lookup, &c.popup), (&d.lookup, &d.popup));
        c.reset(Section::Services);
        assert_eq!(c.online.providers, d.online.providers);
        assert_eq!(
            (c.online.jimaku_api_key.as_str(), c.network.proxy.as_str()),
            ("k", "")
        );
        c.reset(Section::Anki);
        assert_eq!(
            (c.anki.deck.as_str(), c.anki.api_key.as_str()),
            ("Lapis", "a")
        );
        c.reset(Section::WordAudio);
        assert_eq!((c.word_audio.volume, c.word_audio.sources.len()), (100, 1));
        c.reset(Section::Shortcuts);
        assert!(c.shortcuts.is_empty());
        assert_eq!(c.paths, before.paths);
        c.reset(Section::Advanced);
        assert_eq!(c.paths, d.paths);
        assert_eq!(c.player.hwdec, "no");
        // TOML laid over the settings changes its keys alone
        let over = before
            .laid_over("[general]\nui_scale = 125\n[popup]\nmax_height = 300")
            .unwrap();
        assert_eq!(
            (over.general.ui_scale, over.general.theme.as_str()),
            (125, "light")
        );
        assert_eq!(
            (over.popup.max_height, over.popup.max_width),
            (300.0, 900.0)
        );
        // a tab saves its keys alone
        let mut saved = before.clone();
        saved.copy_section(Section::General, &c);
        assert_eq!(saved.general.ui_scale, 100);
        assert_eq!(saved.player.seek_seconds, 3);
        assert_eq!(saved.player.hwdec, "no");
        assert_eq!(saved.subtitles, before.subtitles);
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

    // T-AUD-001, T-AUD-007: the sources in order, on or off; none built in
    #[test]
    fn word_audio_sources_keep_their_order() {
        assert!(Config::default().word_audio.sources.is_empty());
        assert!(Config::default().word_audio.autoplay);
        let text = r#"
[[word_audio.sources]]
name = "Local"
url = "http://localhost:5050/?term={term}&reading={reading}"

[[word_audio.sources]]
name = "Other"
url = "https://audio.example.org/{term}"
enabled = false
"#;
        let c = Config::from_toml(text).unwrap();
        let names: Vec<(&str, bool)> = c
            .word_audio
            .sources
            .iter()
            .map(|s| (s.name.as_str(), s.enabled))
            .collect();
        assert_eq!(names, [("Local", true), ("Other", false)]);
        let again = Config::from_toml(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(again.word_audio, c.word_audio);
    }

    // the settings folder is checked at start
    #[test]
    fn a_folder_that_cannot_be_written_is_told() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        assert!(check_writable(dir.path()).is_ok());
        assert!(check_writable(&dir.path().join("not yet")).is_ok());
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        // root writes anywhere: the check only holds for a user
        if std::fs::write(locked.join("x"), b"").is_err() {
            assert!(check_writable(&locked).is_err());
        }
        assert!(!locked.join(".nikomiru-write-check").exists());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
