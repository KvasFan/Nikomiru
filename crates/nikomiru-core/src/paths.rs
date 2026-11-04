// SPDX-License-Identifier: GPL-3.0-only

//! XDG Base Directory paths of Nikomiru (SPECIFICATION §5.4), implemented here
//! instead of with the `directories` crate, which pulls an MPL-2.0 dependency
//! (SPECIFICATION §3.6).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Name of Nikomiru's subdirectory in every XDG directory.
pub const APP_DIR: &str = "nikomiru";

/// Name of Nikomiru's folder in the profile's `AppData` (ADR 0009).
pub const WINDOWS_APP_DIR: &str = "Nikomiru";

/// Errors while resolving the paths.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathsError {
    /// `HOME` is unset or not absolute, and some XDG variable needs it; on
    /// Windows, `USERPROFILE`, and `APPDATA` or `LOCALAPPDATA` with it.
    #[error("HOME is not set to an absolute path")]
    NoHome,
}

impl crate::Localized for PathsError {
    fn message_id(&self) -> &'static str {
        match self {
            PathsError::NoHome => "error-paths",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            PathsError::NoHome => Vec::new(),
        }
    }
}

/// Nikomiru's directories. Each one already includes the `nikomiru` subdirectory,
/// except [`Paths::pictures_dir`], which is the user's pictures folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `$XDG_CONFIG_HOME/nikomiru`.
    pub config_dir: PathBuf,
    /// `$XDG_DATA_HOME/nikomiru`.
    pub data_dir: PathBuf,
    /// `$XDG_CACHE_HOME/nikomiru`.
    pub cache_dir: PathBuf,
    /// `$XDG_STATE_HOME/nikomiru`.
    pub state_dir: PathBuf,
    /// `$XDG_RUNTIME_DIR`, if set (for the single-instance socket, APP-001).
    pub runtime_dir: Option<PathBuf>,
    /// The user's pictures folder (`XDG_PICTURES_DIR`).
    pub pictures_dir: PathBuf,
}

/// An absolute path from a variable; relative or empty values are invalid and
/// ignored, as the XDG Base Directory specification requires.
fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    let p = PathBuf::from(value?);
    p.is_absolute().then_some(p)
}

/// Reads `XDG_PICTURES_DIR` from `user-dirs.dirs` (format of xdg-user-dirs:
/// `XDG_PICTURES_DIR="$HOME/Pictures"`).
fn pictures_from_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("XDG_PICTURES_DIR=") else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let path = if let Some(rest) = value.strip_prefix("$HOME") {
            home.join(rest.trim_start_matches('/'))
        } else {
            PathBuf::from(value)
        };
        return path.is_absolute().then_some(path);
    }
    None
}

impl Paths {
    /// Paths from the process environment.
    pub fn from_env() -> Result<Self, PathsError> {
        #[cfg(windows)]
        return Self::resolve_windows(|k| std::env::var_os(k));
        #[cfg(not(windows))]
        Self::resolve(|k| std::env::var_os(k), |p| std::fs::read_to_string(p).ok())
    }

    /// The paths on Windows (ADR 0009): the settings in the roaming profile
    /// (`%APPDATA%`), which follows the user; the data, cache and logs in the
    /// local one (`%LOCALAPPDATA%`), each in a folder of its own so that a
    /// backup of the data holds no cache; the pictures in the profile's
    /// `Pictures`. The runtime folder is the local one: the single instance
    /// names its pipe after it.
    pub fn resolve_windows(var: impl Fn(&str) -> Option<OsString>) -> Result<Self, PathsError> {
        let profile = absolute(var("USERPROFILE"));
        let in_profile = |name: &str, fallback: &str| {
            absolute(var(name))
                .or_else(|| profile.as_ref().map(|p| p.join("AppData").join(fallback)))
                .ok_or(PathsError::NoHome)
        };
        let roaming = in_profile("APPDATA", "Roaming")?.join(WINDOWS_APP_DIR);
        let local = in_profile("LOCALAPPDATA", "Local")?.join(WINDOWS_APP_DIR);
        let pictures_dir = match &profile {
            Some(p) => p.join("Pictures"),
            None => local.join("Pictures"),
        };
        Ok(Self {
            config_dir: roaming,
            data_dir: local.join("data"),
            cache_dir: local.join("cache"),
            state_dir: local.join("state"),
            runtime_dir: Some(local),
            pictures_dir,
        })
    }

    /// Paths from an arbitrary environment and file reader (for tests).
    pub fn resolve(
        var: impl Fn(&str) -> Option<OsString>,
        read: impl Fn(&Path) -> Option<String>,
    ) -> Result<Self, PathsError> {
        let home = absolute(var("HOME"));
        let base = |name: &str, fallback: &str| -> Result<PathBuf, PathsError> {
            match absolute(var(name)) {
                Some(p) => Ok(p),
                None => home
                    .as_ref()
                    .map(|h| h.join(fallback))
                    .ok_or(PathsError::NoHome),
            }
        };
        let config_home = base("XDG_CONFIG_HOME", ".config")?;
        let pictures_dir = match absolute(var("XDG_PICTURES_DIR")) {
            Some(p) => p,
            None => {
                let home = home.as_ref().ok_or(PathsError::NoHome)?;
                read(&config_home.join("user-dirs.dirs"))
                    .and_then(|text| pictures_from_user_dirs(&text, home))
                    .unwrap_or_else(|| home.join("Pictures"))
            }
        };
        Ok(Self {
            config_dir: config_home.join(APP_DIR),
            data_dir: base("XDG_DATA_HOME", ".local/share")?.join(APP_DIR),
            cache_dir: base("XDG_CACHE_HOME", ".cache")?.join(APP_DIR),
            state_dir: base("XDG_STATE_HOME", ".local/state")?.join(APP_DIR),
            runtime_dir: absolute(var("XDG_RUNTIME_DIR")),
            pictures_dir,
        })
    }

    /// `config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// `library.redb`.
    pub fn library_db(&self) -> PathBuf {
        self.data_dir.join("library.redb")
    }

    /// Folder of the reading model, fetched on request (MNG-004).
    pub fn ocr_model_dir(&self) -> PathBuf {
        self.data_dir.join("models/manga-ocr")
    }

    /// Folder of the imported dictionaries.
    pub fn dicts_dir(&self) -> PathBuf {
        self.data_dir.join("dicts")
    }

    /// Folder of the log files.
    pub fn logs_dir(&self) -> PathBuf {
        self.state_dir.join("logs")
    }

    /// Single-instance socket (APP-001), if `XDG_RUNTIME_DIR` is set.
    pub fn socket_path(&self) -> Option<PathBuf> {
        self.runtime_dir.as_ref().map(|d| d.join("nikomiru.sock"))
    }

    /// Folder of the mpv option presets (PLY-016).
    pub fn mpv_presets_dir(&self) -> PathBuf {
        self.config_dir.join("mpv")
    }

    /// The user's own `mpv.conf`, which mpv reads and Nikomiru does not
    /// (`config=no`, PLY-005): offered on the Player page (PLY-016).
    pub fn user_mpv_conf(&self) -> PathBuf {
        self.config_dir.parent().map_or_else(
            || self.config_dir.join("mpv.conf"),
            |c| c.join("mpv/mpv.conf"),
        )
    }

    /// Folder of the subtitles downloaded for the videos (MET-008).
    pub fn subtitles_dir(&self) -> PathBuf {
        self.data_dir.join("subtitles")
    }

    /// Folder of the media of the review's cards (REV-001).
    pub fn review_dir(&self) -> PathBuf {
        self.data_dir.join("review")
    }

    /// Folder of the screenshots (PLY-011).
    pub fn screenshots_dir(&self) -> PathBuf {
        self.pictures_dir.join("Nikomiru")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn resolve(vars: &[(&str, &str)], files: &[(&str, &str)]) -> Result<Paths, PathsError> {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        let files: HashMap<PathBuf, String> = files
            .iter()
            .map(|(k, v)| (PathBuf::from(k), (*v).to_string()))
            .collect();
        Paths::resolve(|k| vars.get(k).cloned(), |p| files.get(p).cloned())
    }

    // ADR 0009: which variable makes which folder, under a root that is
    // absolute on this system
    #[test]
    fn windows_folders_come_from_the_profile() {
        let root = if cfg!(windows) { r"C:\u" } else { "/u" };
        let at = |rest: &str| {
            rest.split('/')
                .fold(PathBuf::from(root), |path, name| path.join(name))
        };
        let vars = |pairs: &[(&str, PathBuf)]| {
            let map: HashMap<String, OsString> = pairs
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone().into()))
                .collect();
            Paths::resolve_windows(move |k| map.get(k).cloned())
        };
        let p = vars(&[
            ("USERPROFILE", at("")),
            ("APPDATA", at("roam")),
            ("LOCALAPPDATA", at("local")),
        ])
        .unwrap();
        assert_eq!(p.config_dir, at("roam/Nikomiru"));
        assert_eq!(p.data_dir, at("local/Nikomiru/data"));
        assert_eq!(p.cache_dir, at("local/Nikomiru/cache"));
        assert_eq!(p.state_dir, at("local/Nikomiru/state"));
        assert_eq!(p.pictures_dir, at("Pictures"));
        assert_eq!(p.runtime_dir, Some(at("local/Nikomiru")));
        // the profile alone gives AppData's usual folders
        let p = vars(&[("USERPROFILE", at(""))]).unwrap();
        assert_eq!(p.config_dir, at("AppData/Roaming/Nikomiru"));
        assert_eq!(p.data_dir, at("AppData/Local/Nikomiru/data"));
        // nothing, or a relative profile: no folders
        assert_eq!(vars(&[]), Err(PathsError::NoHome));
        assert_eq!(
            vars(&[("USERPROFILE", PathBuf::from("u"))]),
            Err(PathsError::NoHome)
        );
    }

    #[cfg(unix)]
    #[test]
    fn defaults_come_from_home() {
        let p = resolve(&[("HOME", "/home/u")], &[]).unwrap();
        assert_eq!(
            p.config_file(),
            PathBuf::from("/home/u/.config/nikomiru/config.toml")
        );
        assert_eq!(
            p.library_db(),
            PathBuf::from("/home/u/.local/share/nikomiru/library.redb")
        );
        assert_eq!(p.cache_dir, PathBuf::from("/home/u/.cache/nikomiru"));
        assert_eq!(
            p.logs_dir(),
            PathBuf::from("/home/u/.local/state/nikomiru/logs")
        );
        assert_eq!(
            p.screenshots_dir(),
            PathBuf::from("/home/u/Pictures/Nikomiru")
        );
        assert_eq!(p.socket_path(), None);
        assert_eq!(
            p.user_mpv_conf(),
            PathBuf::from("/home/u/.config/mpv/mpv.conf")
        );
    }

    #[cfg(unix)]
    #[test]
    fn xdg_variables_win_and_relative_ones_are_ignored() {
        let p = resolve(
            &[
                ("HOME", "/home/u"),
                ("XDG_CONFIG_HOME", "/cfg"),
                ("XDG_DATA_HOME", "relative/data"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(p.config_dir, PathBuf::from("/cfg/nikomiru"));
        assert_eq!(p.data_dir, PathBuf::from("/home/u/.local/share/nikomiru"));
        assert_eq!(
            p.socket_path(),
            Some(PathBuf::from("/run/user/1000/nikomiru.sock"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn pictures_folder_from_user_dirs() {
        let p = resolve(
            &[("HOME", "/home/u")],
            &[(
                "/home/u/.config/user-dirs.dirs",
                "# comment\nXDG_PICTURES_DIR=\"$HOME/Immagini\"\n",
            )],
        )
        .unwrap();
        assert_eq!(
            p.screenshots_dir(),
            PathBuf::from("/home/u/Immagini/Nikomiru")
        );
        let p = resolve(&[("HOME", "/home/u"), ("XDG_PICTURES_DIR", "/pics")], &[]).unwrap();
        assert_eq!(p.pictures_dir, PathBuf::from("/pics"));
    }

    #[cfg(unix)]
    #[test]
    fn missing_home_is_an_error_only_when_needed() {
        assert_eq!(resolve(&[], &[]), Err(PathsError::NoHome));
        let all = [
            ("XDG_CONFIG_HOME", "/c"),
            ("XDG_DATA_HOME", "/d"),
            ("XDG_CACHE_HOME", "/k"),
            ("XDG_STATE_HOME", "/s"),
            ("XDG_PICTURES_DIR", "/p"),
        ];
        assert!(resolve(&all, &[]).is_ok());
    }
}
