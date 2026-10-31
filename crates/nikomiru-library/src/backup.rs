// SPDX-License-Identifier: GPL-3.0-only

//! Backups (BAK-001…BAK-003): a ZIP with a manifest, made by `create`. A
//! restore unpacks one beside the data with [`unpack`], and the next start
//! puts it in place with [`finish`], before the library opens.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use crate::db::{Library, LibraryError, SCHEMA_VERSION};

/// The format of a backup.
pub const FORMAT: u32 = 1;
/// The extension of a backup.
pub const EXTENSION: &str = "nikomiru-backup";
/// The most an archive may unpack to.
const TOTAL_LIMIT: u64 = 32 << 30;
/// The most entries in an archive.
const ENTRY_LIMIT: usize = 200_000;

const MANIFEST: &str = "manifest.json";
const CONFIG: &str = "config.toml";
const LIBRARY: &str = "library.redb";
const DICTS: &str = "dicts";
const SUBTITLES: &str = "subtitles";
const PRESETS: &str = "mpv";
const PENDING: &str = "restore-pending";
const BEFORE: &str = "before-restore";
/// Marks an unpacked backup as whole.
const READY: &str = ".ready";

/// What a backup says of itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The format of the archive.
    pub format: u32,
    /// The Nikomiru that wrote it.
    pub version: String,
    /// The library's schema.
    pub schema: u32,
    /// When (Unix ms).
    pub created_at: i64,
    /// It holds the dictionaries.
    pub dictionaries: bool,
    /// It holds keys and passwords (BAK-002).
    pub secrets: bool,
}

/// Why a backup failed.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// Reading or writing a file.
    #[error("{0}")]
    Io(String),
    /// The library.
    #[error(transparent)]
    Library(#[from] LibraryError),
    /// It is not a backup, or it is damaged.
    #[error("not a backup of Nikomiru, or a damaged one")]
    Damaged,
    /// A newer Nikomiru wrote it (DAT-002).
    #[error("a newer version of Nikomiru made this backup")]
    Newer,
    /// An entry would land outside the data, or the sizes are too large.
    #[error("this backup is not safe to restore")]
    Unsafe,
}

impl nikomiru_core::Localized for BackupError {
    fn message_id(&self) -> &'static str {
        match self {
            Self::Io(_) => "error-backup-io",
            Self::Library(_) => "error-backup-library",
            Self::Damaged => "error-backup-damaged",
            Self::Newer => "error-backup-newer",
            Self::Unsafe => "error-backup-unsafe",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Io(reason) => vec![("reason", reason.clone())],
            Self::Library(e) => vec![("reason", e.to_string())],
            Self::Damaged | Self::Newer | Self::Unsafe => Vec::new(),
        }
    }
}

fn io(e: impl std::fmt::Display) -> BackupError {
    BackupError::Io(e.to_string())
}

/// What goes into a backup.
#[derive(Debug)]
pub struct Contents<'a> {
    /// `config.toml`, keys left out or not.
    pub config: String,
    /// The library.
    pub library: &'a Library,
    /// `$XDG_DATA_HOME/nikomiru`.
    pub data_dir: &'a Path,
    /// The mpv presets' folder.
    pub presets_dir: &'a Path,
    /// With the dictionaries.
    pub dictionaries: bool,
    /// Keys and passwords are in `config`.
    pub secrets: bool,
    /// Nikomiru's version.
    pub version: &'a str,
    /// Now (Unix ms).
    pub now: i64,
}

fn add_folder<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    folder: &Path,
    name: &str,
) -> Result<(), BackupError> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Ok(());
    };
    let options = SimpleFileOptions::default().large_file(true);
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let inner = format!("{name}/{file_name}");
        let kind = entry.file_type().map_err(io)?;
        if kind.is_dir() {
            add_folder(zip, &path, &inner)?;
        } else if kind.is_file() {
            zip.start_file(inner, options).map_err(io)?;
            std::io::copy(&mut File::open(&path).map_err(io)?, zip).map_err(io)?;
        }
    }
    Ok(())
}

/// Writes a backup at `out`, whole or not at all, readable only by the user
/// (BAK-002).
pub fn create(contents: &Contents<'_>, out: &Path) -> Result<Manifest, BackupError> {
    let manifest = Manifest {
        format: FORMAT,
        version: contents.version.to_string(),
        schema: contents.library.schema(),
        created_at: contents.now,
        dictionaries: contents.dictionaries,
        secrets: contents.secrets,
    };
    let part = out.with_extension("part");
    let snapshot = out.with_extension("library");
    let written = (|| {
        contents.library.copy_to(&snapshot)?;
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&part)
                .map_err(io)?
        };
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().large_file(true);
        zip.start_file(MANIFEST, options).map_err(io)?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(io)?)
            .map_err(io)?;
        zip.start_file(CONFIG, options).map_err(io)?;
        zip.write_all(contents.config.as_bytes()).map_err(io)?;
        zip.start_file(LIBRARY, options).map_err(io)?;
        std::io::copy(&mut File::open(&snapshot).map_err(io)?, &mut zip).map_err(io)?;
        add_folder(&mut zip, &contents.data_dir.join(SUBTITLES), SUBTITLES)?;
        add_folder(&mut zip, contents.presets_dir, PRESETS)?;
        if contents.dictionaries {
            add_folder(&mut zip, &contents.data_dir.join(DICTS), DICTS)?;
        }
        zip.finish().map_err(io)?;
        std::fs::rename(&part, out).map_err(io)
    })();
    let _ = std::fs::remove_file(&snapshot);
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written.map(|()| manifest)
}

/// What the backup at `path` says, once it is known it can be restored.
pub fn inspect(path: &Path) -> Result<Manifest, BackupError> {
    let file = File::open(path).map_err(io)?;
    let mut zip = ZipArchive::new(file).map_err(|_| BackupError::Damaged)?;
    let manifest: Manifest = {
        let entry = zip.by_name(MANIFEST).map_err(|_| BackupError::Damaged)?;
        let mut text = Vec::new();
        entry
            .take(1 << 20)
            .read_to_end(&mut text)
            .map_err(|_| BackupError::Damaged)?;
        serde_json::from_slice(&text).map_err(|_| BackupError::Damaged)?
    };
    if manifest.format > FORMAT || manifest.schema > SCHEMA_VERSION {
        return Err(BackupError::Newer);
    }
    if zip.by_name(LIBRARY).is_err() || zip.by_name(CONFIG).is_err() {
        return Err(BackupError::Damaged);
    }
    Ok(manifest)
}

/// Unpacks a backup into the folder the next start takes it from (BAK-003).
/// Names are checked, and sizes counted as they are read: an archive's own
/// figures are not believed. Returns the `config.toml` unpacked, for the keys
/// left out to be put back.
pub fn unpack(archive: &Path, data_dir: &Path) -> Result<PathBuf, BackupError> {
    inspect(archive)?;
    let pending = data_dir.join(PENDING);
    let _ = std::fs::remove_dir_all(&pending);
    let result = (|| {
        std::fs::create_dir_all(&pending).map_err(io)?;
        let mut zip =
            ZipArchive::new(File::open(archive).map_err(io)?).map_err(|_| BackupError::Damaged)?;
        if zip.len() > ENTRY_LIMIT {
            return Err(BackupError::Unsafe);
        }
        let mut left = TOTAL_LIMIT;
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(|_| BackupError::Damaged)?;
            let Some(name) = entry.enclosed_name() else {
                return Err(BackupError::Unsafe);
            };
            let root = name.components().next().map(|c| c.as_os_str().to_owned());
            let known = root.as_deref().is_some_and(|r| {
                [MANIFEST, CONFIG, LIBRARY, DICTS, SUBTITLES, PRESETS]
                    .iter()
                    .any(|k| r == std::ffi::OsStr::new(k))
            });
            if !known || entry.is_symlink() {
                return Err(BackupError::Unsafe);
            }
            let path = pending.join(&name);
            if entry.is_dir() {
                std::fs::create_dir_all(&path).map_err(io)?;
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            let mut out = File::create(&path).map_err(io)?;
            let copied = std::io::copy(&mut entry.take(left + 1), &mut out).map_err(io)?;
            if copied > left {
                return Err(BackupError::Unsafe);
            }
            left -= copied;
        }
        File::create(pending.join(READY)).map_err(io)?;
        Ok(pending.join(CONFIG))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&pending);
    }
    result
}

fn replace(from: &Path, to: &Path, keep: &Path) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        std::fs::rename(to, keep)?;
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(from, to)
}

/// Puts an unpacked backup in place, if there is one, before anything opens
/// the files (BAK-003): what was there goes to `before-restore`, the last one
/// only. Dictionaries stay unless the backup has them. Whether one was put.
pub fn finish(data_dir: &Path, config_file: &Path, presets_dir: &Path) -> std::io::Result<bool> {
    let pending = data_dir.join(PENDING);
    if !pending.join(READY).exists() {
        let _ = std::fs::remove_dir_all(&pending);
        return Ok(false);
    }
    let before = data_dir.join(BEFORE);
    let _ = std::fs::remove_dir_all(&before);
    std::fs::create_dir_all(&before)?;
    replace(&pending.join(CONFIG), config_file, &before.join(CONFIG))?;
    replace(
        &pending.join(LIBRARY),
        &data_dir.join(LIBRARY),
        &before.join(LIBRARY),
    )?;
    replace(
        &pending.join(SUBTITLES),
        &data_dir.join(SUBTITLES),
        &before.join(SUBTITLES),
    )?;
    replace(&pending.join(PRESETS), presets_dir, &before.join(PRESETS))?;
    replace(
        &pending.join(DICTS),
        &data_dir.join(DICTS),
        &before.join(DICTS),
    )?;
    std::fs::remove_dir_all(&pending)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoRecord;

    struct Fixture {
        dir: tempfile::TempDir,
        data: PathBuf,
        presets: PathBuf,
        library: Library,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let presets = dir.path().join("mpv");
        std::fs::create_dir_all(data.join("subtitles/abc")).unwrap();
        std::fs::create_dir_all(data.join("dicts/one")).unwrap();
        std::fs::create_dir_all(&presets).unwrap();
        std::fs::write(data.join("subtitles/abc/x.ja.srt"), "sub").unwrap();
        std::fs::write(data.join("dicts/one/dict.redb"), "dict").unwrap();
        std::fs::write(presets.join("a.conf"), "preset").unwrap();
        let library = Library::open(&data.join(LIBRARY)).unwrap();
        let record = VideoRecord {
            path: "/v/ep".into(),
            last_position_ms: 1234,
            ..VideoRecord::default()
        };
        library.put_video("video/ep", &record).unwrap();
        Fixture {
            dir,
            data,
            presets,
            library,
        }
    }

    fn backup(f: &Fixture, dictionaries: bool) -> PathBuf {
        let out = f.dir.path().join("b.nikomiru-backup");
        create(
            &Contents {
                config: "[general]\n".into(),
                library: &f.library,
                data_dir: &f.data,
                presets_dir: &f.presets,
                dictionaries,
                secrets: false,
                version: "1.2.0",
                now: 5,
            },
            &out,
        )
        .unwrap();
        out
    }

    fn names(path: &Path) -> Vec<String> {
        let zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
        zip.file_names()
            .map(|n| n.map(|n| n.into_owned()).unwrap_or_default())
            .collect()
    }

    // T-BAK-001, T-BAK-002
    #[test]
    fn a_backup_holds_what_it_says() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let out = backup(&f, false);
        let list = names(&out);
        for name in [
            MANIFEST,
            CONFIG,
            LIBRARY,
            "subtitles/abc/x.ja.srt",
            "mpv/a.conf",
        ] {
            assert!(list.iter().any(|n| n == name), "{name}: {list:?}");
        }
        assert!(!list.iter().any(|n| n.starts_with("dicts")), "{list:?}");
        assert!(
            names(&backup(&f, true))
                .iter()
                .any(|n| n.starts_with("dicts/"))
        );
        let mode = std::fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let manifest = inspect(&out).unwrap();
        assert_eq!((manifest.created_at, manifest.schema), (5, SCHEMA_VERSION));
        assert!(!f.dir.path().join("b.library").exists());
    }

    // T-BAK-003: a round trip, with what was there kept
    #[test]
    fn a_restore_gives_back_the_library() {
        let f = fixture();
        let out = backup(&f, false);
        let mut changed = f.library.video("video/ep").unwrap().unwrap();
        changed.last_position_ms = 9;
        f.library.put_video("video/ep", &changed).unwrap();
        let config = f.dir.path().join("config.toml");
        std::fs::write(&config, "old").unwrap();
        let unpacked = unpack(&out, &f.data).unwrap();
        assert_eq!(std::fs::read_to_string(unpacked).unwrap(), "[general]\n");
        let Fixture {
            dir,
            data,
            presets,
            library,
            ..
        } = f;
        drop(library);
        assert!(finish(&data, &config, &presets).unwrap());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "[general]\n");
        assert_eq!(
            std::fs::read_to_string(data.join("before-restore/config.toml")).unwrap(),
            "old"
        );
        // the dictionaries are not in it, and stay
        assert!(data.join("dicts/one/dict.redb").exists());
        assert!(!data.join("restore-pending").exists());
        let library = Library::open(&data.join(LIBRARY)).unwrap();
        assert_eq!(
            library.video("video/ep").unwrap().unwrap().last_position_ms,
            1234
        );
        assert!(!finish(&data, &config, &presets).unwrap());
        drop(dir);
    }

    // T-BAK-003: half an unpacking is dropped
    #[test]
    fn an_unfinished_restore_changes_nothing() {
        let f = fixture();
        let pending = f.data.join(PENDING);
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::write(pending.join(LIBRARY), "half").unwrap();
        let config = f.dir.path().join("config.toml");
        assert!(!finish(&f.data, &config, &f.presets).unwrap());
        assert!(!pending.exists());
        assert!(f.library.video("video/ep").unwrap().is_some());
    }

    fn archive(path: &Path, manifest: &Manifest, extra: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file(MANIFEST, options).unwrap();
        zip.write_all(&serde_json::to_vec(manifest).unwrap())
            .unwrap();
        zip.start_file(CONFIG, options).unwrap();
        zip.start_file(LIBRARY, options).unwrap();
        for (name, bytes) in extra {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn manifest(schema: u32) -> Manifest {
        Manifest {
            format: FORMAT,
            version: "9".into(),
            schema,
            created_at: 0,
            dictionaries: false,
            secrets: false,
        }
    }

    // T-BAK-003, NFR-SEC-02
    #[test]
    fn nothing_lands_outside_the_data() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let path = dir.path().join("b.zip");
        for name in ["../evil", "/etc/evil", "other/file", "subtitles/../../evil"] {
            archive(&path, &manifest(1), &[(name, b"x")]);
            assert!(
                matches!(unpack(&path, &data), Err(BackupError::Unsafe)),
                "{name}"
            );
            assert!(!dir.path().join("evil").exists());
            assert!(!data.join(PENDING).exists());
        }
    }

    // T-BAK-003
    #[test]
    fn a_newer_backup_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.zip");
        archive(&path, &manifest(SCHEMA_VERSION + 1), &[]);
        assert!(matches!(inspect(&path), Err(BackupError::Newer)));
        std::fs::write(&path, "not a zip").unwrap();
        assert!(matches!(inspect(&path), Err(BackupError::Damaged)));
    }
}
