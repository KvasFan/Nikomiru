// SPDX-License-Identifier: GPL-3.0-only

//! `config.toml` (SPECIFICATION §5.4, §9). Unknown keys are kept as they are and
//! written back, so a file from a newer version or edited by hand loses nothing.
//! Writes are atomic and the file is readable only by the user (0600).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Errors while reading or writing the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The configuration file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML or has values of the wrong type.
    #[error("{path}: {source}")]
    Parse {
        /// The configuration file.
        path: PathBuf,
        /// The parser's error, with line and column.
        source: toml::de::Error,
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
        Ok(c)
    }

    /// Reads `path`; a missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes `path` atomically with permissions 0600.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        crate::fs::write_atomic(path, text.as_bytes(), 0o600).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Dotted names of the keys this version does not know (for a warning).
    pub fn unknown_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = self.other.keys().cloned().collect();
        out.extend(self.general.other.keys().map(|k| format!("general.{k}")));
        out.extend(self.player.other.keys().map(|k| format!("player.{k}")));
        out.sort();
        out
    }
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
deck = "Lapis"

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
            ["anki", "future_section", "general.new_flag", "player.later"]
        );
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
        std::fs::write(&path, "[player]\nseek_seconds = \"three\"\n").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
        assert!(err.to_string().contains("config.toml"));
    }

    // T-SEC-008
    #[test]
    fn saved_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("anikoto/config.toml");
        Config::default().save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }
}
