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

/// The words the cards of Anki's decks say are known or being learned, by
/// headword (KNW-005): read again from Anki, so neither backed up nor
/// synced; a status set by hand wins over them.
const ANKI_WORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("anki_words");

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

/// The largest word list read (KNW-006).
pub const MAX_LIST_BYTES: u64 = 16 << 20;

impl WordStatus {
    /// Its name in a word list.
    pub fn name(self) -> &'static str {
        match self {
            WordStatus::Unknown => "unknown",
            WordStatus::Learning => "learning",
            WordStatus::Known => "known",
            WordStatus::Ignored => "ignored",
        }
    }

    /// A status from its name, in any case.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "unknown" | "new" => Some(WordStatus::Unknown),
            "learning" => Some(WordStatus::Learning),
            "known" => Some(WordStatus::Known),
            "ignored" | "ignore" => Some(WordStatus::Ignored),
            _ => None,
        }
    }
}

/// The words of a list (KNW-006): a line is `word`, known with any
/// reading, or `word,reading,status` (a comma or a tab between); a heading
/// line and empty lines are skipped, and so is a line whose status is not
/// one of the four.
pub fn parse_list(text: &str) -> Vec<(String, String, WordStatus)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split([',', '\t']).map(str::trim).collect();
        let (word, reading, status) = match parts.as_slice() {
            [word] => (*word, "", Some(WordStatus::Known)),
            [word, reading] => (*word, *reading, Some(WordStatus::Known)),
            [word, reading, status, ..] => (*word, *reading, WordStatus::from_name(status)),
            [] => continue,
        };
        if word.is_empty() || word.eq_ignore_ascii_case("expression") || word == "word" {
            continue;
        }
        if let Some(status) = status {
            out.push((word.to_string(), reading.to_string(), status));
        }
    }
    out
}

/// The words marked as a list that [`parse_list`] reads back: a heading,
/// then `word,reading,status`, by word.
pub fn write_list(marks: &HashMap<String, WordMark>) -> String {
    let mut rows: Vec<(&str, &str, WordStatus)> = marks
        .iter()
        .filter_map(|(key, mark)| {
            let (word, reading) = key.split_once('\n')?;
            Some((word, reading, mark.status))
        })
        .collect();
    rows.sort();
    let mut out = String::from("expression,reading,status\n");
    for (word, reading, status) in rows {
        // a comma would cut the line: such a word is not a word
        if word.contains([',', '\t']) || reading.contains([',', '\t']) {
            continue;
        }
        out.push_str(&format!("{word},{reading},{}\n", status.name()));
    }
    out
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

    /// Sets the statuses of many words at once (KNW-006): one transaction.
    pub fn set_word_statuses(
        &self,
        marks: &[(String, String, WordMark)],
    ) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(WORD_STATUS))?;
            for (expression, reading, mark) in marks {
                let bytes = postcard::to_stdvec(mark)?;
                db_err!(t.insert(word_key(expression, reading).as_str(), bytes.as_slice()))?;
            }
        }
        db_err!(tx.commit())
    }

    /// Replaces the words read from Anki (KNW-005).
    pub fn set_anki_words(&self, words: &HashMap<String, WordStatus>) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let _ = tx.delete_table(ANKI_WORDS);
            let mut t = db_err!(tx.open_table(ANKI_WORDS))?;
            for (word, status) in words {
                let bytes = postcard::to_stdvec(status)?;
                db_err!(t.insert(word.as_str(), bytes.as_slice()))?;
            }
        }
        db_err!(tx.commit())
    }

    /// The words last read from Anki (KNW-005).
    pub fn anki_words(&self) -> Result<HashMap<String, WordStatus>, LibraryError> {
        self.read(|tx| match tx.open_table(ANKI_WORDS) {
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

    // T-KNW-005, T-KNW-006: many statuses at once; the words read from
    // Anki replaced as a whole
    #[test]
    fn lists_and_anki_words_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let known = WordMark {
            status: WordStatus::Known,
            changed_at: 1,
        };
        lib.set_word_statuses(&[
            ("猫".into(), String::new(), known),
            ("犬".into(), "いぬ".into(), known),
        ])
        .unwrap();
        assert_eq!(lib.word_statuses().unwrap().len(), 2);
        assert!(lib.anki_words().unwrap().is_empty());
        let first = HashMap::from([
            ("食べる".to_string(), WordStatus::Known),
            ("飲む".to_string(), WordStatus::Learning),
        ]);
        lib.set_anki_words(&first).unwrap();
        assert_eq!(lib.anki_words().unwrap(), first);
        let second = HashMap::from([("見る".to_string(), WordStatus::Learning)]);
        lib.set_anki_words(&second).unwrap();
        assert_eq!(lib.anki_words().unwrap(), second, "the old ones are gone");
    }

    // T-KNW-006: a list of words, plain or with readings and statuses, and
    // the list written read back the same
    #[test]
    fn word_lists_are_read_and_written() {
        let text = "\u{feff}expression,reading,status\n猫\n犬\tいぬ\n食べる,たべる,learning\n\n# note\n見る,みる,LEARNING\n寝る,ねる,mature\n";
        let words = parse_list(text);
        assert_eq!(
            words,
            [
                ("猫".to_string(), String::new(), WordStatus::Known),
                ("犬".to_string(), "いぬ".to_string(), WordStatus::Known),
                (
                    "食べる".to_string(),
                    "たべる".to_string(),
                    WordStatus::Learning
                ),
                ("見る".to_string(), "みる".to_string(), WordStatus::Learning),
            ]
        );
        let marks: HashMap<String, WordMark> = words
            .iter()
            .map(|(w, r, status)| {
                let mark = WordMark {
                    status: *status,
                    changed_at: 0,
                };
                (word_key(w, r), mark)
            })
            .collect();
        let written = write_list(&marks);
        assert!(written.starts_with("expression,reading,status\n"));
        let mut again = parse_list(&written);
        again.sort();
        let mut words = words;
        words.sort();
        assert_eq!(again, words);
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
