// SPDX-License-Identifier: GPL-3.0-only

//! XDG Base Directory paths of Nikomiru (SPECIFICATION §5.4), implemented here
//! instead of with the `directories` crate, which pulls an MPL-2.0 dependency
//! (SPECIFICATION §3.6).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Name of Nikomiru's subdirectory in every XDG directory.
pub const APP_DIR: &str = "nikomiru";

/// Errors while resolving the paths.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathsError {
    /// `HOME` is unset or not absolute, and some XDG variable needs it.
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
        Self::resolve(|k| std::env::var_os(k), |p| std::fs::read_to_string(p).ok())
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

    /// Folder of the subtitles downloaded for the videos (MET-008).
    pub fn subtitles_dir(&self) -> PathBuf {
        self.data_dir.join("subtitles")
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
    }

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
