// SPDX-License-Identifier: GPL-3.0-only

//! `library.redb` (SPECIFICATION §5.4, §8): the videos, and from schema 2 the
//! series and the imported folders (M5).
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
pub const SCHEMA_VERSION: u32 = 2;

pub(crate) const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
pub(crate) const VIDEOS: TableDefinition<&str, &[u8]> = TableDefinition::new("videos");
/// Series by id (schema 2, LIB-002).
pub(crate) const SERIES: TableDefinition<u64, &[u8]> = TableDefinition::new("series");
/// Imported folders by path (schema 2, LIB-002).
pub(crate) const FOLDERS: TableDefinition<&str, &[u8]> = TableDefinition::new("folders");

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
        $e.map_err(|e| $crate::db::LibraryError::Db(e.into()))
    };
}
pub(crate) use db_err;

/// A subtitle chosen for a video or a series (SUB-008, LIB-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubtitleChoice {
    /// A file: next to the video, downloaded or opened by hand.
    File(String),
    /// An embedded track, found again by its index, else by its language and
    /// title (another episode numbers its tracks its own way).
    Embedded {
        /// Its FFmpeg index in the file it was chosen in.
        ff_index: Option<i64>,
        /// Its language tag.
        language: Option<String>,
        /// Its title.
        title: Option<String>,
    },
    /// No subtitles.
    Off,
}

/// How an imported folder is grouped (LIB-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Grouping {
    /// By the series the file names give (LIB-003).
    #[default]
    BySeries,
    /// One collection per folder.
    ByFolder,
}

/// What a series remembers for its episodes (LIB-010); an episode's own
/// values win.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SeriesPrefs {
    /// The audio track, by language.
    pub audio_language: Option<String>,
    /// The main subtitle.
    pub subtitle: Option<SubtitleChoice>,
    /// The subtitle offset.
    pub delay_ms: Option<i64>,
}

/// A series, or a collection of a folder (§8).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SeriesRecord {
    /// Its name, as shown; the user may rename it (LIB-011).
    pub title: String,
    /// What a scan matches it by: the grouping key ([`crate::grouping::Group::key`]).
    pub key: String,
    /// The grouping that made it.
    pub grouping: Grouping,
    /// The imported folder it comes from.
    pub root: String,
    /// When it was added (Unix ms).
    pub added_at: i64,
    /// Deleted by the user: a scan does not make it again (LIB-004).
    pub deleted: bool,
    /// Its cover image, once made (LIB-005).
    pub cover: Option<String>,
    /// What its episodes inherit (LIB-010).
    pub prefs: SeriesPrefs,
    /// Merged by the user into this series (LIB-011): a scan puts its
    /// episodes there.
    pub merged_into: Option<u64>,
}

/// An imported folder (LIB-002).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderRecord {
    /// How it is grouped.
    pub grouping: Grouping,
    /// When it was added (Unix ms).
    pub added_at: i64,
    /// Its last scan (Unix ms).
    pub scanned_at: i64,
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
    /// The series it belongs to (schema 2, LIB-002).
    pub series: Option<u64>,
    /// The season, from its name or folder (LIB-003) or set by hand.
    pub season: Option<u32>,
    /// The episode, from its name or its siblings (LIB-003) or set by hand.
    pub episode: Option<u32>,
    /// The main subtitle chosen for it (SUB-008, LIB-010).
    pub subtitle: Option<SubtitleChoice>,
    /// The secondary subtitle chosen for it (SUB-017).
    pub secondary: Option<SubtitleChoice>,
    /// Its own subtitle offset (LIB-010).
    pub delay_ms: Option<i64>,
    /// Moved by the user to this series (LIB-011): a scan with the series'
    /// grouping keeps it there.
    pub moved_to: Option<u64>,
    /// Removed from the library by the user: a scan does not bring it back
    /// (LIB-012). The file stays.
    pub removed: bool,
}

/// The video record of schema 1, read once by the migration to schema 2.
#[derive(Deserialize)]
struct VideoRecordV1 {
    path: String,
    size: u64,
    mtime: i64,
    duration_ms: Option<u64>,
    last_position_ms: u64,
    last_played_at: Option<i64>,
    completed_at: Option<i64>,
    aid: Option<i64>,
    volume: Option<f64>,
    speed: Option<f64>,
}

impl From<VideoRecordV1> for VideoRecord {
    fn from(v: VideoRecordV1) -> Self {
        Self {
            path: v.path,
            size: v.size,
            mtime: v.mtime,
            duration_ms: v.duration_ms,
            last_position_ms: v.last_position_ms,
            last_played_at: v.last_played_at,
            completed_at: v.completed_at,
            aid: v.aid,
            volume: v.volume,
            speed: v.speed,
            ..Self::default()
        }
    }
}

/// A migration from schema `from` to `from + 1`. It MUST be idempotent.
pub type Migration = fn(&redb::WriteTransaction) -> Result<(), LibraryError>;

/// Schema 1 → 2: the videos rewritten with the fields of M5, the series and
/// folder tables made. A record already rewritten is kept as it is.
fn migrate_1(tx: &redb::WriteTransaction) -> Result<(), LibraryError> {
    let mut videos = db_err!(tx.open_table(VIDEOS))?;
    let mut rows: Vec<(String, Vec<u8>)> = Vec::new();
    for row in db_err!(videos.iter())? {
        let (k, v) = db_err!(row)?;
        rows.push((k.value().to_string(), v.value().to_vec()));
    }
    for (uid, bytes) in rows {
        if postcard::from_bytes::<VideoRecord>(&bytes).is_ok() {
            continue;
        }
        let record: VideoRecord = postcard::from_bytes::<VideoRecordV1>(&bytes)?.into();
        db_err!(videos.insert(uid.as_str(), postcard::to_stdvec(&record)?.as_slice()))?;
    }
    db_err!(tx.open_table(SERIES))?;
    db_err!(tx.open_table(FOLDERS))?;
    Ok(())
}

/// The migrations of this version, by starting schema.
const MIGRATIONS: &[(u32, Migration)] = &[(1, migrate_1)];

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

/// The tables of the current schema, made empty.
fn create_tables(tx: &redb::WriteTransaction) -> Result<(), LibraryError> {
    db_err!(tx.open_table(VIDEOS))?;
    db_err!(tx.open_table(SERIES))?;
    db_err!(tx.open_table(FOLDERS))?;
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
                create_tables(&tx)?;
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
        create_tables(&tx)?;
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

    /// Saves the playback position of a video, if it exists (LIB-006), and
    /// its duration when known; the first time it reaches 90% it is watched
    /// (LIB-008).
    pub fn save_position(
        &self,
        uid: &str,
        position_ms: u64,
        duration_ms: Option<u64>,
        at_ms: i64,
    ) -> Result<(), LibraryError> {
        if let Some(mut rec) = self.video(uid)? {
            rec.last_position_ms = position_ms;
            rec.last_played_at = Some(at_ms);
            if let Some(d) = duration_ms.filter(|d| *d > 0) {
                rec.duration_ms = Some(d);
            }
            crate::policy::mark_watched(&mut rec, at_ms);
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
            lib.save_position(&uid, 61_000, None, 1_700_000_000_000)
                .unwrap();
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

    // T-DAT-001: schema 1 → 2, the videos written by 0.0.0
    #[test]
    fn a_schema_1_library_keeps_its_videos() {
        #[derive(Serialize)]
        struct V1 {
            path: String,
            size: u64,
            mtime: i64,
            duration_ms: Option<u64>,
            last_position_ms: u64,
            last_played_at: Option<i64>,
            completed_at: Option<i64>,
            aid: Option<i64>,
            volume: Option<f64>,
            speed: Option<f64>,
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.redb");
        drop(Library::open_with(&path, 1, &[]).unwrap());
        {
            let db = redb::Database::open(&path).unwrap();
            let tx = db.begin_write().unwrap();
            {
                let mut t = tx.open_table(VIDEOS).unwrap();
                let old = V1 {
                    path: "/v/Ep01.mkv".into(),
                    size: 10,
                    mtime: 20,
                    duration_ms: Some(1_440_000),
                    last_position_ms: 61_000,
                    last_played_at: Some(5),
                    completed_at: None,
                    aid: Some(2),
                    volume: Some(80.0),
                    speed: Some(1.25),
                };
                t.insert("video/Ep01", postcard::to_stdvec(&old).unwrap().as_slice())
                    .unwrap();
            }
            tx.commit().unwrap();
        }
        for _ in 0..2 {
            let lib = Library::open(&path).unwrap();
            assert_eq!(lib.schema(), 2);
            let v = lib.video("video/Ep01").unwrap().unwrap();
            assert_eq!(
                (
                    v.path.as_str(),
                    v.last_position_ms,
                    v.aid,
                    v.speed,
                    v.series
                ),
                ("/v/Ep01.mkv", 61_000, Some(2), Some(1.25), None)
            );
            assert!(lib.series_list().unwrap().is_empty());
            assert!(lib.folders().unwrap().is_empty());
        }
        assert!(dir.path().join("library.redb.bak-v1").exists());
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
