// SPDX-License-Identifier: GPL-3.0-only

//! The books added (BOK-004, BOK-005), in a table of their own (§8 `books`):
//! a library without it is read as one with no books. A book keeps its file
//! where it is, as a video does (LIB-012), and is known by what it holds, so
//! a moved file keeps its place.

use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::{Library, LibraryError, db_err};

/// By uid.
const BOOKS: TableDefinition<&str, &[u8]> = TableDefinition::new("books");

/// A book's uid, from what it holds.
pub fn book_uid(title: &str, author: &str, chars: u64) -> String {
    let digest = Sha256::digest(format!("{title}\n{author}\n{chars}").as_bytes());
    let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("book/{hex}")
}

/// A book of the library.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BookRecord {
    /// Its file.
    pub path: String,
    /// Its title.
    pub title: String,
    /// Its author.
    pub author: String,
    /// Its characters.
    pub chars: u64,
    /// The characters before the place reached (BOK-005).
    pub read_chars: u64,
    /// The chapter reached.
    pub chapter: u32,
    /// The paragraph reached in it.
    pub paragraph: u32,
    /// When it was last read (Unix ms).
    pub last_read_at: Option<i64>,
}

impl Library {
    /// Every book, the last read first.
    pub fn books(&self) -> Result<Vec<(String, BookRecord)>, LibraryError> {
        let mut out: Vec<(String, BookRecord)> = self.read(|tx| match tx.open_table(BOOKS) {
            Ok(t) => {
                let mut out = Vec::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.push((k.value().to_string(), postcard::from_bytes(v.value())?));
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(Vec::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })?;
        out.sort_by_key(|(_, b)| std::cmp::Reverse(b.last_read_at));
        Ok(out)
    }

    /// One book.
    pub fn book(&self, uid: &str) -> Result<Option<BookRecord>, LibraryError> {
        self.read(|tx| match tx.open_table(BOOKS) {
            Ok(t) => match db_err!(t.get(uid))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Saves a book.
    pub fn put_book(&self, uid: &str, record: &BookRecord) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(BOOKS))?;
            let bytes = postcard::to_allocvec(record)?;
            db_err!(t.insert(uid, bytes.as_slice()))?;
        }
        db_err!(tx.commit())
    }

    /// Forgets a book: its file stays (LIB-012).
    pub fn remove_book(&self, uid: &str) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(BOOKS))?;
            db_err!(t.remove(uid))?;
        }
        db_err!(tx.commit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-BOK-004
    #[test]
    fn a_book_is_kept_and_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert!(lib.books().unwrap().is_empty());
        let (old, new) = (book_uid("a", "b", 1), book_uid("c", "d", 2));
        assert_ne!(old, new);
        assert_eq!(
            old,
            book_uid("a", "b", 1),
            "the same book, wherever its file"
        );
        let record = |at| BookRecord {
            title: "t".into(),
            last_read_at: Some(at),
            ..BookRecord::default()
        };
        lib.put_book(&old, &record(1)).unwrap();
        lib.put_book(&new, &record(2)).unwrap();
        let list = lib.books().unwrap();
        assert_eq!(list[0].0, new, "the last read first");
        assert_eq!(lib.book(&old).unwrap(), Some(record(1)));
        lib.remove_book(&old).unwrap();
        assert_eq!(lib.book(&old).unwrap(), None);
    }
}
