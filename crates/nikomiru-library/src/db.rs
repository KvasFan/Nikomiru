// SPDX-License-Identifier: GPL-3.0-only

//! `library.redb` (SPECIFICATION §5.4, §8). M1 has only the `videos` table.
//!
//! - DAT-001: schema migrations go forward only, are numbered and idempotent;
//!   before migrating, the file is copied to `library.redb.bak-v<N>`.
//! - DAT-002: a file written by a newer version (schema above the supported one)
//!   is opened read-only and never modified.

use std::path::{Path, PathBuf};

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::uid;

/// Schema version written by this version of Nikomiru.
pub const SCHEMA_VERSION: u32 = 1;

const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const VIDEOS: TableDefinition<&str, &[u8]> = TableDefinition::new("videos");

/// Errors of the library database.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    /// Error from redb.
    #[error("library database: {0}")]
    Db(#[from] redb::Error),
    /// A record could not be decoded or encoded.
    #[error("library record: {0}")]
    Record(#[from] postcard::Error),
    /// File system error (backup before a migration).
    #[error("{path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file comes from a newer version: it is open read-only (DAT-002).
    #[error("the library was written by a newer version of Nikomiru (schema {0}); it is read-only")]
    ReadOnly(u32),
    /// The `meta` table is damaged.
    #[error("the library has no valid schema version")]
    BadSchema,
}

impl nikomiru_core::Localized for LibraryError {
    fn message_id(&self) -> &'static str {
        match self {
            LibraryError::Db(_) => "error-library",
            LibraryError::Record(_) => "error-library-record",
            LibraryError::Io { .. } => "error-library-io",
            LibraryError::ReadOnly(_) => "error-library-read-only",
            LibraryError::BadSchema => "error-library-schema",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            LibraryError::Db(e) => vec![("reason", e.to_string())],
            LibraryError::Record(e) => vec![("reason", e.to_string())],
            LibraryError::Io { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            LibraryError::ReadOnly(_) | LibraryError::BadSchema => Vec::new(),
        }
    }
}

macro_rules! db_err {
    ($e:expr) => {
        $e.map_err(|e| LibraryError::Db(e.into()))
    };
}

/// A video of the library (§8; the fields of later milestones are added by
/// migrations).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VideoRecord {
    /// Absolute path of the file.
    pub path: String,
    /// File size in bytes, to recognize a moved file (DAT-003).
    pub size: u64,
    /// Modification time (Unix seconds).
    pub mtime: i64,
    /// Duration in ms, once known.
    pub duration_ms: Option<u64>,
    /// Saved playback position in ms (LIB-006).
    pub last_position_ms: u64,
    /// Last time the file was played (Unix ms).
    pub last_played_at: Option<i64>,
    /// First time the position reached 90% (Unix ms, LIB-008).
    pub completed_at: Option<i64>,
    /// Chosen audio track (mpv `aid`, PLY-003).
    pub aid: Option<i64>,
    /// Volume in percent.
    pub volume: Option<f64>,
    /// Playback speed.
    pub speed: Option<f64>,
}

/// A migration from schema `from` to `from + 1`. It MUST be idempotent.
pub type Migration = fn(&redb::WriteTransaction) -> Result<(), LibraryError>;

/// The migrations of this version, by starting schema (none yet: schema 1).
const MIGRATIONS: &[(u32, Migration)] = &[];

enum Handle {
    ReadWrite(redb::Database),
    ReadOnly(redb::ReadOnlyDatabase),
}

/// The open library.
pub struct Library {
    db: Handle,
    schema: u32,
}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Library")
            .field("schema", &self.schema)
            .field("read_only", &self.read_only())
            .finish()
    }
}

fn read_schema(db: &impl ReadableDatabase) -> Result<Option<u32>, LibraryError> {
    let tx = db_err!(db.begin_read())?;
    let table = match tx.open_table(META) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(LibraryError::Db(e.into())),
    };
    let Some(v) = db_err!(table.get("schema_version"))? else {
        return Ok(None);
    };
    let bytes: [u8; 4] = v.value().try_into().map_err(|_| LibraryError::BadSchema)?;
    Ok(Some(u32::from_le_bytes(bytes)))
}

fn write_schema(tx: &redb::WriteTransaction, version: u32) -> Result<(), LibraryError> {
    let mut meta = db_err!(tx.open_table(META))?;
    db_err!(meta.insert("schema_version", version.to_le_bytes().as_slice()))?;
    db_err!(meta.insert("created_with", env!("CARGO_PKG_VERSION").as_bytes()))?;
    Ok(())
}

/// Repairs a library that was not closed: a copy first
/// (`library.redb.bak-repair`), then a read-write open, which repairs, and a
/// clean close.
fn repair(path: &Path) -> Result<(), LibraryError> {
    tracing::warn!("{} was not closed cleanly: repairing it", path.display());
    let backup = path.with_file_name(format!(
        "{}.bak-repair",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    std::fs::copy(path, &backup).map_err(|source| LibraryError::Io {
        path: backup.clone(),
        source,
    })?;
    drop(db_err!(redb::Database::open(path))?);
    tracing::info!(
        "library repaired; the copy before the repair is {}",
        backup.display()
    );
    Ok(())
}

impl Library {
    /// Opens or creates the library at `path`.
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        Self::open_with(path, SCHEMA_VERSION, MIGRATIONS)
    }

    /// Like [`Library::open`] with an explicit schema and migration list (tests).
    pub fn open_with(
        path: &Path,
        current: u32,
        migrations: &[(u32, Migration)],
    ) -> Result<Self, LibraryError> {
        if path.exists() {
            let found = match redb::Builder::new().open_read_only(path) {
                Ok(ro) => read_schema(&ro)?,
                // A process that ended without closing the file (crash,
                // SIGKILL, an exit that skipped the drop) leaves it to be
                // repaired, and a read-only open cannot repair (REL-001).
                Err(redb::DatabaseError::RepairAborted) => {
                    repair(path)?;
                    let ro = db_err!(redb::Builder::new().open_read_only(path))?;
                    read_schema(&ro)?
                }
                Err(e) => return Err(LibraryError::Db(e.into())),
            };
            if let Some(v) = found
                && v > current
            {
                // DAT-002: never touch a file from a newer version.
                let ro = db_err!(redb::Builder::new().open_read_only(path))?;
                return Ok(Self {
                    db: Handle::ReadOnly(ro),
                    schema: v,
                });
            }
            let from = found.unwrap_or(0);
            if from > 0 && from < current {
                let backup = path.with_file_name(format!(
                    "{}.bak-v{from}",
                    path.file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default()
                ));
                std::fs::copy(path, &backup).map_err(|source| LibraryError::Io {
                    path: backup.clone(),
                    source,
                })?;
            }
            let db = db_err!(redb::Database::open(path))?;
            for v in from..current {
                if v == 0 {
                    continue;
                }
                let tx = db_err!(db.begin_write())?;
                if let Some((_, m)) = migrations.iter().find(|(f, _)| *f == v) {
                    m(&tx)?;
                }
                write_schema(&tx, v + 1)?;
                db_err!(tx.commit())?;
            }
            if from == 0 {
                let tx = db_err!(db.begin_write())?;
                write_schema(&tx, current)?;
                db_err!(tx.open_table(VIDEOS))?;
                db_err!(tx.commit())?;
            }
            return Ok(Self {
                db: Handle::ReadWrite(db),
                schema: current,
            });
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| LibraryError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let db = db_err!(redb::Database::create(path))?;
        let tx = db_err!(db.begin_write())?;
        write_schema(&tx, current)?;
        db_err!(tx.open_table(VIDEOS))?;
        db_err!(tx.commit())?;
        Ok(Self {
            db: Handle::ReadWrite(db),
            schema: current,
        })
    }

    /// Whether the library is read-only (DAT-002).
    pub fn read_only(&self) -> bool {
        matches!(self.db, Handle::ReadOnly(_))
    }

    /// Schema version of the file.
    pub fn schema(&self) -> u32 {
        self.schema
    }

    pub(crate) fn read<T>(
        &self,
        f: impl FnOnce(&redb::ReadTransaction) -> Result<T, LibraryError>,
    ) -> Result<T, LibraryError> {
        let tx = match &self.db {
            Handle::ReadWrite(db) => db_err!(db.begin_read())?,
            Handle::ReadOnly(db) => db_err!(db.begin_read())?,
        };
        f(&tx)
    }

    pub(crate) fn write(&self) -> Result<redb::WriteTransaction, LibraryError> {
        match &self.db {
            Handle::ReadWrite(db) => db_err!(db.begin_write()),
            Handle::ReadOnly(_) => Err(LibraryError::ReadOnly(self.schema)),
        }
    }

    /// The video with this uid.
    pub fn video(&self, uid: &str) -> Result<Option<VideoRecord>, LibraryError> {
        self.read(|tx| {
            let t = db_err!(tx.open_table(VIDEOS))?;
            match db_err!(t.get(uid))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            }
        })
    }

    /// Writes (inserts or replaces) a video.
    pub fn put_video(&self, uid: &str, record: &VideoRecord) -> Result<(), LibraryError> {
        let bytes = postcard::to_stdvec(record)?;
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(VIDEOS))?;
            db_err!(t.insert(uid, bytes.as_slice()))?;
        }
        db_err!(tx.commit())
    }

    /// Saves only the playback position of a video, if it exists (LIB-006).
    pub fn save_position(
        &self,
        uid: &str,
        position_ms: u64,
        at_ms: i64,
    ) -> Result<(), LibraryError> {
        if let Some(mut rec) = self.video(uid)? {
            rec.last_position_ms = position_ms;
            rec.last_played_at = Some(at_ms);
            self.put_video(uid, &rec)?;
        }
        Ok(())
    }

    /// The uid of the file at `path`: the existing one if the path is known,
    /// otherwise a new unique uid (DAT-003).
    pub fn uid_for_path(&self, path: &Path) -> Result<String, LibraryError> {
        let owners: Vec<(String, String)> = self.read(|tx| {
            let t = db_err!(tx.open_table(VIDEOS))?;
            let mut out = Vec::new();
            for row in db_err!(t.iter())? {
                let (k, v) = db_err!(row)?;
                let rec: VideoRecord = postcard::from_bytes(v.value())?;
                out.push((k.value().to_string(), rec.path));
            }
            Ok(out)
        })?;
        let mine = path.to_string_lossy();
        if let Some((uid, _)) = owners.iter().find(|(_, p)| *p == mine) {
            return Ok(uid.clone());
        }
        Ok(uid::unique_uid(path, |u| {
            owners.iter().find(|(k, _)| k == u).map(|(_, p)| p.clone())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str) -> VideoRecord {
        VideoRecord {
            path: path.into(),
            size: 10,
            mtime: 20,
            ..VideoRecord::default()
        }
    }

    const CHILD_PATH: &str = "NIKOMIRU_TEST_LIBRARY_LEFT_OPEN";

    /// Runs only as the child of the next test: writes and exits without
    /// closing the library, as an app that dies would.
    #[test]
    fn child_leaves_the_library_open() {
        let Ok(path) = std::env::var(CHILD_PATH) else {
            return;
        };
        let lib = Library::open(Path::new(&path)).unwrap();
        lib.put_video("a", &record("/v/a.mkv")).unwrap();
        std::mem::forget(lib);
        std::process::exit(0);
    }

    // T-REL-001
    #[test]
    fn a_library_left_open_is_repaired_with_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.redb");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["db::tests::child_leaves_the_library_open", "--exact"])
            .env(CHILD_PATH, &path)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            matches!(
                redb::Builder::new().open_read_only(&path),
                Err(redb::DatabaseError::RepairAborted)
            ),
            "the child left the file to be repaired"
        );
        let lib = Library::open(&path).unwrap();
        assert!(!lib.read_only());
        assert_eq!(lib.video("a").unwrap().unwrap().path, "/v/a.mkv");
        assert!(dir.path().join("library.redb.bak-repair").exists());
    }

    #[test]
    fn create_store_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/library.redb");
        {
            let lib = Library::open(&path).unwrap();
            assert_eq!(lib.schema(), SCHEMA_VERSION);
            let uid = lib.uid_for_path(Path::new("/v/Ep01.mkv")).unwrap();
            assert_eq!(uid, "video/Ep01");
            lib.put_video(&uid, &record("/v/Ep01.mkv")).unwrap();
            lib.save_position(&uid, 61_000, 1_700_000_000_000).unwrap();
        }
        let lib = Library::open(&path).unwrap();
        let rec = lib.video("video/Ep01").unwrap().unwrap();
        assert_eq!(rec.last_position_ms, 61_000);
        assert_eq!(rec.last_played_at, Some(1_700_000_000_000));
        assert_eq!(
            lib.uid_for_path(Path::new("/v/Ep01.mkv")).unwrap(),
            "video/Ep01"
        );
        assert_eq!(
            lib.uid_for_path(Path::new("/w/Ep01.mkv")).unwrap(),
            "video/Ep01 (2)"
        );
    }

    fn add_marker(tx: &redb::WriteTransaction) -> Result<(), LibraryError> {
        let mut meta = db_err!(tx.open_table(META))?;
        // idempotent: inserting the same key twice gives the same state
        db_err!(meta.insert("migrated_to_2", b"yes".as_slice()))?;
        Ok(())
    }

    // T-DAT-001
    #[test]
    fn migration_makes_a_backup_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.redb");
        Library::open_with(&path, 1, &[])
            .unwrap()
            .put_video("video/a", &record("/a.mkv"))
            .unwrap();
        let migrations: &[(u32, Migration)] = &[(1, add_marker)];
        let lib = Library::open_with(&path, 2, migrations).unwrap();
        assert_eq!(lib.schema(), 2);
        assert!(dir.path().join("library.redb.bak-v1").exists());
        assert_eq!(lib.video("video/a").unwrap().unwrap().path, "/a.mkv");
        drop(lib);
        // a second open does nothing more
        let lib = Library::open_with(&path, 2, migrations).unwrap();
        assert_eq!(lib.schema(), 2);
        assert!(!dir.path().join("library.redb.bak-v2").exists());
    }

    // T-DAT-002
    #[test]
    fn a_newer_schema_is_opened_read_only_and_never_modified() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.redb");
        Library::open_with(&path, 7, &[])
            .unwrap()
            .put_video("video/a", &record("/a.mkv"))
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let lib = Library::open(&path).unwrap();
        assert!(lib.read_only());
        assert_eq!(lib.schema(), 7);
        assert_eq!(lib.video("video/a").unwrap().unwrap().path, "/a.mkv");
        assert!(matches!(
            lib.put_video("video/b", &record("/b.mkv")),
            Err(LibraryError::ReadOnly(7))
        ));
        drop(lib);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use nikomiru_core::Localized;
        let errors = [
            LibraryError::Db(redb::Error::Corrupted("x".into())),
            LibraryError::Record(postcard::Error::DeserializeUnexpectedEnd),
            LibraryError::Io {
                path: PathBuf::from("/l/library.redb"),
                source: std::io::Error::other("x"),
            },
            LibraryError::ReadOnly(9),
            LibraryError::BadSchema,
        ];
        let all: Vec<&dyn Localized> = errors.iter().map(|e| e as _).collect();
        let dir = std::path::Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
