// SPDX-License-Identifier: GPL-3.0-only

//! Books from the interface (SPECIFICATION §6.16): their list, the book open,
//! and reading a file off the interface's thread.

use std::path::PathBuf;

use nikomiru_books::Book;
use nikomiru_library::books::BookRecord;

use crate::i18n::Translator;

/// The book open.
#[derive(Debug)]
pub struct OpenBook {
    /// Its uid.
    pub uid: String,
    /// The book.
    pub book: Book,
    /// The chapter shown.
    pub chapter: usize,
    /// The paragraph at the top of the page.
    pub paragraph: usize,
    /// The view must go to that paragraph (on opening, or a new chapter).
    pub jump: bool,
    /// When reading started (monotonic ms), and how far along.
    pub started: (i64, u64),
}

/// The Books page.
#[derive(Debug, Default)]
pub struct BooksPage {
    /// The books.
    pub list: Vec<(String, BookRecord)>,
    /// The book open.
    pub open: Option<OpenBook>,
    /// A book is being read.
    pub busy: bool,
    /// The page's message, and whether it is good news.
    pub status: Option<(String, bool)>,
}

/// What reading a file came to.
pub type Read = Result<Book, String>;

/// Reads `path` on a thread of its own; `done` gets the book, or why not.
pub fn start_read(
    path: PathBuf,
    translator: Translator,
    done: Box<dyn FnOnce(PathBuf, Read) + Send>,
) {
    let spawned = std::thread::Builder::new()
        .name("book".into())
        .spawn(move || {
            let read = nikomiru_books::read(&path).map_err(|e| translator.error(&e));
            done(path, read);
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start the book thread: {e}");
    }
}

/// The share of a book read, in percent.
pub fn percent(record: &BookRecord) -> u64 {
    (record.read_chars * 100)
        .checked_div(record.chars)
        .unwrap_or(0)
        .min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-BOK-005
    #[test]
    fn the_share_read() {
        let record = |read_chars, chars| BookRecord {
            read_chars,
            chars,
            ..BookRecord::default()
        };
        assert_eq!(percent(&record(1, 4)), 25);
        assert_eq!(percent(&record(0, 0)), 0);
        assert_eq!(percent(&record(9, 4)), 100);
    }
}
