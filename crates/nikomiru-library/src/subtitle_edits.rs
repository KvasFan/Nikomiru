// SPDX-License-Identifier: GPL-3.0-only

//! Subtitle lines edited by hand (SUB-063), in a table of their own (§8
//! `subtitle_edits`): by the subtitle's own key, which its loader makes from
//! the lines as read, then by the line's place in the file. The file itself
//! is never written; a library without the table has no edits.

use std::collections::BTreeMap;

use redb::TableDefinition;
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

/// By the subtitle's key.
pub(crate) const SUBTITLE_EDITS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("subtitle_edits");

/// The longest text of an edited line, in bytes.
pub const MAX_LINE_TEXT: usize = 4096;
/// The most lines edited in one subtitle.
pub const MAX_EDITS: usize = 20_000;

/// A line as the user wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineEdit {
    /// Start, in milliseconds, without the subtitle's offset.
    pub start_ms: i64,
    /// End, in milliseconds.
    pub end_ms: i64,
    /// The text, lines separated by `\n`.
    pub text: String,
}

impl LineEdit {
    /// Whether it can be kept: times in order, some text, not too long.
    pub fn is_valid(&self) -> bool {
        self.start_ms >= 0
            && self.end_ms > self.start_ms
            && !self.text.trim().is_empty()
            && self.text.len() <= MAX_LINE_TEXT
    }
}

impl Library {
    /// The lines edited in the subtitle of `key`, by their place in the file.
    pub fn subtitle_edits(&self, key: &str) -> Result<BTreeMap<u32, LineEdit>, LibraryError> {
        self.read(|tx| match tx.open_table(SUBTITLE_EDITS) {
            Ok(t) => match db_err!(t.get(key))? {
                Some(v) => Ok(postcard::from_bytes(v.value())?),
                None => Ok(BTreeMap::new()),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(BTreeMap::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Keeps the line at `order` as edited, or as in the file again with
    /// `None`. An edit that is not valid, or one more than [`MAX_EDITS`],
    /// is refused with `false`.
    pub fn set_subtitle_edit(
        &self,
        key: &str,
        order: u32,
        edit: Option<LineEdit>,
    ) -> Result<bool, LibraryError> {
        let mut edits = self.subtitle_edits(key)?;
        match edit {
            Some(edit) => {
                if !edit.is_valid() || (edits.len() >= MAX_EDITS && !edits.contains_key(&order)) {
                    return Ok(false);
                }
                edits.insert(order, edit);
            }
            None => {
                edits.remove(&order);
            }
        }
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(SUBTITLE_EDITS))?;
            if edits.is_empty() {
                db_err!(t.remove(key))?;
            } else {
                db_err!(t.insert(key, postcard::to_stdvec(&edits)?.as_slice()))?;
            }
        }
        db_err!(tx.commit())?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start_ms: i64, text: &str) -> LineEdit {
        LineEdit {
            start_ms,
            end_ms: start_ms + 1_000,
            text: text.into(),
        }
    }

    // T-SUB-063
    #[test]
    fn edited_lines_are_kept_by_subtitle_and_place() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert!(lib.subtitle_edits("k").unwrap().is_empty(), "no table yet");
        assert!(
            lib.set_subtitle_edit("k", 3, Some(line(10, "今日は")))
                .unwrap()
        );
        assert!(lib.set_subtitle_edit("k", 1, Some(line(0, "あ"))).unwrap());
        assert!(
            lib.set_subtitle_edit("other", 3, Some(line(5, "別")))
                .unwrap()
        );
        let edits = lib.subtitle_edits("k").unwrap();
        assert_eq!(edits.keys().copied().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(edits[&3], line(10, "今日は"));
        // edited again, then back to the file's
        assert!(
            lib.set_subtitle_edit("k", 3, Some(line(20, "明日")))
                .unwrap()
        );
        assert_eq!(lib.subtitle_edits("k").unwrap()[&3].text, "明日");
        assert!(lib.set_subtitle_edit("k", 3, None).unwrap());
        assert!(lib.set_subtitle_edit("k", 1, None).unwrap());
        assert!(lib.subtitle_edits("k").unwrap().is_empty());
        assert_eq!(lib.subtitle_edits("other").unwrap().len(), 1);
        // refused: times out of order, no text, too long
        for bad in [
            LineEdit {
                end_ms: 10,
                ..line(10, "x")
            },
            LineEdit {
                start_ms: -1,
                ..line(0, "x")
            },
            line(0, "  \n "),
            line(0, &"あ".repeat(MAX_LINE_TEXT)),
        ] {
            assert!(!lib.set_subtitle_edit("k", 0, Some(bad)).unwrap());
        }
        assert!(lib.subtitle_edits("k").unwrap().is_empty());
    }
}
