// SPDX-License-Identifier: GPL-3.0-only

//! What the learner knows of each word (KNW-001), in the `word_status`
//! table of `library.redb`, by headword and reading. A word never marked
//! has no row; a word marked back to unknown keeps one, with its time, so
//! that the newest change wins when devices sync (BAK-005). A new table
//! leaves the other records as they are, so the schema version does not
//! change (ADR 0007 §26).

use std::collections::HashMap;

use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

pub(crate) const WORD_STATUS: TableDefinition<&str, &[u8]> = TableDefinition::new("word_status");

/// What the learner knows of a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WordStatus {
    /// Not known yet.
    Unknown,
    /// Being learned: a card was made, or the learner said so.
    Learning,
    /// Known.
    Known,
    /// Left out of the counts: a name, a sound, noise.
    Ignored,
}

/// A word's status and when it was set (Unix ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordMark {
    /// The status.
    pub status: WordStatus,
    /// When it was set.
    pub changed_at: i64,
}

impl WordMark {
    /// Which of two marks of a word wins: the newer, then, so that every
    /// device picks the same, the greater status.
    pub fn wins_over(&self, other: &WordMark) -> bool {
        (self.changed_at, self.status) > (other.changed_at, other.status)
    }
}

/// The key of a word: its headword and its reading, as the popup groups
/// them (LKP-036).
pub fn word_key(expression: &str, reading: &str) -> String {
    format!("{expression}\n{reading}")
}

impl Library {
    /// Sets the status of a word.
    pub fn set_word_status(
        &self,
        expression: &str,
        reading: &str,
        mark: WordMark,
    ) -> Result<(), LibraryError> {
        let bytes = postcard::to_stdvec(&mark)?;
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(WORD_STATUS))?;
            db_err!(t.insert(word_key(expression, reading).as_str(), bytes.as_slice()))?;
        }
        db_err!(tx.commit())
    }

    /// Every word marked, by [`word_key`].
    pub fn word_statuses(&self) -> Result<HashMap<String, WordMark>, LibraryError> {
        self.read(|tx| match tx.open_table(WORD_STATUS) {
            Ok(t) => {
                let mut out = HashMap::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.insert(k.value().to_string(), postcard::from_bytes(v.value())?);
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(HashMap::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-KNW-001: statuses kept by headword and reading, the last one set
    #[test]
    fn statuses_are_kept_by_word() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert!(lib.word_statuses().unwrap().is_empty(), "no table yet");
        let mark = |status, changed_at| WordMark { status, changed_at };
        lib.set_word_status("生", "せい", mark(WordStatus::Known, 1))
            .unwrap();
        lib.set_word_status("生", "なま", mark(WordStatus::Learning, 2))
            .unwrap();
        lib.set_word_status("生", "せい", mark(WordStatus::Unknown, 3))
            .unwrap();
        let all = lib.word_statuses().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[&word_key("生", "せい")], mark(WordStatus::Unknown, 3));
        assert_eq!(all[&word_key("生", "なま")], mark(WordStatus::Learning, 2));
        assert_eq!(lib.schema(), crate::db::SCHEMA_VERSION, "no migration");
    }

    // T-KNW-001: the newer mark wins, and a tie is settled the same way
    // everywhere
    #[test]
    fn the_newer_mark_wins() {
        let known = WordMark {
            status: WordStatus::Known,
            changed_at: 5,
        };
        let learning = WordMark {
            status: WordStatus::Learning,
            changed_at: 7,
        };
        assert!(learning.wins_over(&known));
        assert!(!known.wins_over(&learning));
        let tie = WordMark {
            changed_at: 7,
            ..known
        };
        assert!(tie.wins_over(&learning) && !learning.wins_over(&tie));
    }
}
