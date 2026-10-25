// SPDX-License-Identifier: GPL-3.0-only

//! The word audio of a database of the Local Audio Server for Yomichan
//! (AUD-003, [ADR 0004]): `entries(expression, reading, source, file, …)`
//! and `android(file, source, data)`. The exact pair first, then the
//! expression alone; among the files found, the sub-source preferred.
//!
//! After Fushi `fushi/lib/src/utils/misc/local_audio_db.dart` (commit
//! `ac8034e`). The file is read with [`crate::sqlite`], never written: the
//! indexes the real file ships with are used, and without them a lookup
//! reads the whole table.
//!
//! [ADR 0004]: ../../../docs/adr/0004-local-audio-db.md

use std::path::Path;

use crate::sqlite::{Database, SqliteError, Value, columns};

/// The sub-sources preferred when the settings name none (AUD-003); the
/// others come after, in the order found.
pub const DEFAULT_PRIORITY: [&str; 3] = ["nhk16", "forvo", "jpod"];

/// Why a database cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum LocalAudioError {
    /// The file cannot be read as a database.
    #[error(transparent)]
    Sqlite(#[from] SqliteError),
    /// It is not a database of the Local Audio Server.
    #[error("not a database of the Local Audio Server for Yomichan")]
    NotAudio,
}

/// A table: its root page and its columns.
#[derive(Debug, Clone)]
struct Table {
    root: u32,
    columns: Vec<String>,
    rowid: Option<usize>,
}

impl Table {
    fn position(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(name))
    }

    /// The value of column `i` of a row.
    fn value(&self, rowid: i64, row: &[Value], i: usize) -> Value {
        if self.rowid == Some(i) {
            return Value::Integer(rowid);
        }
        row.get(i).cloned().unwrap_or(Value::Null)
    }
}

/// An index usable for a lookup: its root page and how many of its first
/// columns are the ones looked for.
#[derive(Debug, Clone, Copy)]
struct Index {
    root: u32,
    width: usize,
}

/// An audio file of the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFile {
    /// Its name in the database (`file`).
    pub file: String,
    /// Its sub-source (`source`): `nhk16`, `forvo`, …
    pub source: String,
}

/// A database of the Local Audio Server, open read-only.
#[derive(Debug)]
pub struct LocalAudio {
    db: Database,
    entries: Table,
    android: Table,
    by_word: Option<Index>,
    by_file: Option<Index>,
}

/// The longest list read for a word.
const MAX_FILES: usize = 64;

impl LocalAudio {
    /// Opens `path`: an SQLite file with the two tables.
    pub fn open(path: &Path) -> Result<Self, LocalAudioError> {
        let db = Database::open(path)?;
        let schema = db.schema()?;
        let table = |name: &str| {
            schema
                .iter()
                .find(|e| e.kind == "table" && e.name.eq_ignore_ascii_case(name))
                .map(|e| {
                    let cols = columns(&e.sql);
                    Table {
                        root: e.root,
                        rowid: cols.iter().position(|(_, rowid)| *rowid),
                        columns: cols.into_iter().map(|(n, _)| n).collect(),
                    }
                })
        };
        let (Some(entries), Some(android)) = (table("entries"), table("android")) else {
            return Err(LocalAudioError::NotAudio);
        };
        let needs = |t: &Table, names: &[&str]| names.iter().all(|n| t.position(n).is_some());
        if !needs(&entries, &["expression", "reading", "source", "file"])
            || !needs(&android, &["file", "source", "data"])
        {
            return Err(LocalAudioError::NotAudio);
        }
        // an index on (a, b, …), whatever its name; not a partial one
        let index = |table: &str, wanted: &[&str]| {
            schema
                .iter()
                .filter(|e| {
                    e.kind == "index"
                        && e.table.eq_ignore_ascii_case(table)
                        && !e.sql.to_uppercase().contains(" WHERE ")
                })
                .filter_map(|e| {
                    let cols = columns(&e.sql);
                    let width = cols
                        .iter()
                        .zip(wanted)
                        .take_while(|((c, _), w)| c.eq_ignore_ascii_case(w))
                        .count();
                    (width > 0 && e.root > 0).then_some(Index {
                        root: e.root,
                        width,
                    })
                })
                .max_by_key(|i| i.width)
        };
        let by_word = index("entries", &["expression", "reading"]);
        let by_file = index("android", &["file", "source"]);
        if by_word.is_none() || by_file.is_none() {
            tracing::info!("{}: no index, lookups read the tables", path.display());
        }
        let audio = Self {
            db,
            entries,
            android,
            by_word,
            by_file,
        };
        // a file of the right shape with no audio in it is not usable
        let mut any = false;
        audio.db.scan(audio.android.root, &mut |_, _| {
            any = true;
            false
        })?;
        if !any {
            return Err(LocalAudioError::NotAudio);
        }
        Ok(audio)
    }

    /// The rows of `table` whose columns `names` equal `values`, through
    /// `index` (its record ends with the rowid) or a scan.
    fn rows(
        &self,
        table: &Table,
        index: Option<Index>,
        names: &[&str],
        values: &[Value],
    ) -> Result<Vec<(i64, Vec<Value>)>, SqliteError> {
        let positions: Vec<usize> = names.iter().filter_map(|n| table.position(n)).collect();
        let matches = |rowid: i64, row: &[Value]| {
            positions
                .iter()
                .zip(values)
                .all(|(i, v)| table.value(rowid, row, *i) == *v)
        };
        let mut out = Vec::new();
        match index {
            Some(index) => {
                let width = index.width.min(values.len());
                for key in self
                    .db
                    .index_find(index.root, &values[..width], MAX_FILES * 4)?
                {
                    let Some(Value::Integer(rowid)) = key.last().cloned() else {
                        continue;
                    };
                    if let Some(row) = self.db.row(table.root, rowid)?
                        && matches(rowid, &row)
                    {
                        out.push((rowid, row));
                    }
                }
            }
            None => self.db.scan(table.root, &mut |rowid, row| {
                if matches(rowid, &row) {
                    out.push((rowid, row));
                }
                out.len() < MAX_FILES * 4
            })?,
        }
        Ok(out)
    }

    /// The audio files of a word (AUD-003): those of the exact pair, else of
    /// the expression alone; the preferred sub-sources first (`priority`,
    /// [`DEFAULT_PRIORITY`] when empty), each file once.
    pub fn files(
        &self,
        expression: &str,
        reading: &str,
        priority: &[String],
    ) -> Result<Vec<AudioFile>, SqliteError> {
        let text = |s: &str| Value::Text(s.to_string());
        let names = ["expression", "reading"];
        let mut rows = if reading.is_empty() {
            Vec::new()
        } else {
            self.rows(
                &self.entries,
                self.by_word,
                &names,
                &[text(expression), text(reading)],
            )?
        };
        if rows.is_empty() {
            rows = self.rows(
                &self.entries,
                self.by_word,
                &names[..1],
                &[text(expression)],
            )?;
        }
        let (Some(f), Some(s)) = (
            self.entries.position("file"),
            self.entries.position("source"),
        ) else {
            return Ok(Vec::new());
        };
        let mut files: Vec<AudioFile> = Vec::new();
        for (rowid, row) in rows {
            let (Value::Text(file), Value::Text(source)) = (
                self.entries.value(rowid, &row, f),
                self.entries.value(rowid, &row, s),
            ) else {
                continue;
            };
            let found = AudioFile { file, source };
            if !files.contains(&found) {
                files.push(found);
            }
        }
        let rank = |source: &str| {
            if priority.is_empty() {
                DEFAULT_PRIORITY.iter().position(|p| *p == source)
            } else {
                priority.iter().position(|p| p == source)
            }
            .unwrap_or(usize::MAX)
        };
        // stable: the files of one sub-source keep the order of the file
        files.sort_by_key(|a| rank(&a.source));
        files.truncate(MAX_FILES);
        Ok(files)
    }

    /// The audio bytes of a file.
    pub fn audio(&self, file: &AudioFile) -> Result<Option<Vec<u8>>, SqliteError> {
        let values = [
            Value::Text(file.file.clone()),
            Value::Text(file.source.clone()),
        ];
        let rows = self.rows(&self.android, self.by_file, &["file", "source"], &values)?;
        let Some(d) = self.android.position("data") else {
            return Ok(None);
        };
        Ok(rows
            .into_iter()
            .find_map(|(rowid, row)| match self.android.value(rowid, &row, d) {
                Value::Blob(b) => Some(b),
                _ => None,
            }))
    }
}
