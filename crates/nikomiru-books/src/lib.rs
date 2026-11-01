// SPDX-License-Identifier: GPL-3.0-only

//! EPUB books read into clean text (BOK-001…BOK-003): the chapters of the
//! spine, each a list of paragraphs, their furigana apart from the text, as
//! the lookup needs it. A book is untrusted: its archive is read within
//! limits, and its scripts, styles and links are simply not read. A CBZ
//! comes out as a book of pages (MNG-001).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod cbz;
mod epub;
mod markup;
#[cfg(feature = "test-support")]
pub mod sample;
pub mod vertical;

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;

/// Reads the book at `path`: a folder of pictures, a CBZ by its extension,
/// else an EPUB.
pub fn read(path: &Path) -> Result<Book, BookError> {
    if path.is_dir() {
        return cbz::read_folder(path);
    }
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    match extension.as_deref() {
        Some("cbz" | "zip") => cbz::read(path),
        _ => epub::read(path),
    }
}

/// Furigana: the bytes of the paragraph's text it goes over, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ruby {
    /// Its base.
    pub range: Range<usize>,
    /// Its reading.
    pub text: String,
}

/// A paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph {
    /// Its text, without furigana.
    pub text: String,
    /// Its furigana.
    pub ruby: Vec<Ruby>,
    /// It is a heading.
    pub heading: bool,
    /// It is a picture instead: its key in [`Book::images`] (BOK-010).
    pub image: Option<String>,
}

/// A file of the spine with text in it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chapter {
    /// Its first heading, if it has one.
    pub title: Option<String>,
    /// Its paragraphs.
    pub paragraphs: Vec<Paragraph>,
}

/// A book.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Book {
    /// Its title.
    pub title: String,
    /// Its author.
    pub author: String,
    /// It reads from right to left: vertical, unless the user says otherwise
    /// (BOK-009).
    pub right_to_left: bool,
    /// It is made of pages: each chapter is one picture, and a position
    /// counts pages (MNG-001).
    pub manga: bool,
    /// Its chapters, in reading order.
    pub chapters: Vec<Chapter>,
    /// The bytes of its pictures, by their place in the archive.
    pub images: BTreeMap<String, Vec<u8>>,
}

impl Book {
    /// Characters before chapter `chapter`, paragraph `paragraph`: the
    /// position kept for it, which does not change with the font.
    pub fn chars_before(&self, chapter: usize, paragraph: usize) -> u64 {
        if self.manga {
            return chapter.min(self.chapters.len()) as u64;
        }
        let count = |p: &Paragraph| p.text.chars().count() as u64;
        let earlier: u64 = self
            .chapters
            .iter()
            .take(chapter)
            .flat_map(|c| &c.paragraphs)
            .map(count)
            .sum();
        let here: u64 = self
            .chapters
            .get(chapter)
            .map(|c| c.paragraphs.iter().take(paragraph).map(count).sum())
            .unwrap_or(0);
        earlier + here
    }

    /// Every character.
    pub fn chars(&self) -> u64 {
        self.chars_before(self.chapters.len(), 0)
    }
}

/// Why a book cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookError {
    /// Reading the file.
    #[error("{0}")]
    Io(String),
    /// It is not an EPUB, or it is damaged.
    #[error("not an EPUB: {0}")]
    NotEpub(String),
    /// Past the limits of an archive.
    #[error("the EPUB is too large")]
    TooLarge,
    /// It has no text: a book of pictures.
    #[error("the EPUB has no text")]
    NoText,
    /// It is not a CBZ, or it is damaged.
    #[error("not a CBZ: {0}")]
    NotCbz(String),
    /// An archive with no pictures.
    #[error("the CBZ has no pages")]
    NoPages,
}

impl nikomiru_core::Localized for BookError {
    fn message_id(&self) -> &'static str {
        match self {
            Self::Io(_) => "error-book-io",
            Self::NotEpub(_) => "error-book-format",
            Self::TooLarge => "error-book-too-large",
            Self::NoText => "error-book-no-text",
            Self::NotCbz(_) => "error-book-cbz",
            Self::NoPages => "error-book-no-pages",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Io(reason) | Self::NotEpub(reason) | Self::NotCbz(reason) => {
                vec![("reason", reason.clone())]
            }
            Self::TooLarge | Self::NoText | Self::NoPages => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ERR-001
    #[test]
    fn every_error_has_its_message() {
        let errors = [
            BookError::Io("r".into()),
            BookError::NotEpub("r".into()),
            BookError::TooLarge,
            BookError::NoText,
            BookError::NotCbz("r".into()),
            BookError::NoPages,
        ];
        let refs: Vec<&dyn nikomiru_core::Localized> = errors
            .iter()
            .map(|e| e as &dyn nikomiru_core::Localized)
            .collect();
        let i18n = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/i18n");
        let problems = nikomiru_core::error::message_problems(&i18n, &refs);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    // T-BOK-005
    #[test]
    fn a_position_counts_characters() {
        let paragraph = |text: &str| Paragraph {
            text: text.into(),
            ..Paragraph::default()
        };
        let book = Book {
            chapters: vec![
                Chapter {
                    title: None,
                    paragraphs: vec![paragraph("一二"), paragraph("三")],
                },
                Chapter {
                    title: None,
                    paragraphs: vec![paragraph("四五六")],
                },
            ],
            ..Book::default()
        };
        assert_eq!(book.chars_before(0, 1), 2);
        assert_eq!(book.chars_before(1, 0), 3);
        assert_eq!(book.chars(), 6);
    }
}
