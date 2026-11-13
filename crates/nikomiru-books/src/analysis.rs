// SPDX-License-Identifier: GPL-3.0-only

//! A bounded, cancellable text-only reader for whole-book scores (KNW-016).
//! Normal reading keeps its larger limits and pictures. Analysis never reads
//! image entries or opens manga folders/archives.

use crate::{Book, BookError, Chapter};
use nikomiru_core::Cancel;
use std::path::Path;

/// Largest source file on disk considered for analysis.
pub const MAX_FILE_BYTES: u64 = 32 << 20;
/// Largest decompressed chapter/metadata file read for analysis.
pub const MAX_ENTRY_BYTES: u64 = 4 << 20;
/// Decompressed source text budget, including markup and metadata.
pub const MAX_SOURCE_BYTES: u64 = 16 << 20;
/// Largest number of spine entries considered.
pub const MAX_CHAPTERS: usize = 512;
/// Largest amount of clean paragraph text counted as one book.
pub const MAX_TEXT_BYTES: usize = 2 << 20;
/// Most nonempty text paragraphs in an analysed book.
pub const MAX_PARAGRAPHS: usize = 8192;
/// Largest paragraph sent to segmentation, as in the text reader.
pub const MAX_PARAGRAPH_BYTES: usize = 64 << 10;

pub(crate) fn check_cancel(cancel: &Cancel) -> Result<(), BookError> {
    if cancel.is_cancelled() {
        Err(BookError::Io("cancelled".into()))
    } else {
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Budget {
    bytes: usize,
    paragraphs: usize,
}

impl Budget {
    pub(crate) fn accept(&mut self, chapter: &Chapter) -> Result<(), BookError> {
        for p in chapter
            .paragraphs
            .iter()
            .filter(|p| p.image.is_none() && !p.text.trim().is_empty())
        {
            self.bytes = self.bytes.saturating_add(p.text.len());
            self.paragraphs += 1;
            if p.text.len() > MAX_PARAGRAPH_BYTES
                || self.bytes > MAX_TEXT_BYTES
                || self.paragraphs > MAX_PARAGRAPHS
            {
                return Err(BookError::TooLarge);
            }
        }
        Ok(())
    }
}

/// Reads only local EPUB/HTML text within analysis bounds. It never loads
/// pictures, fetches a saved web page again or changes the saved reading place.
/// A cancelled read returns an I/O error; the worker checks its token before
/// presenting any error. Manga and image-only EPUBs return `NoText`.
pub fn read_text(path: &Path, cancel: &Cancel) -> Result<Book, BookError> {
    check_cancel(cancel)?;
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    if path.is_dir() || matches!(extension.as_deref(), Some("cbz" | "zip")) {
        return Err(BookError::NoText);
    }
    let metadata = std::fs::metadata(path).map_err(|e| BookError::Io(e.to_string()))?;
    if !metadata.is_file() {
        return Err(BookError::NoText);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(BookError::TooLarge);
    }
    let book = match extension.as_deref() {
        Some("html" | "htm" | "xhtml") => {
            if metadata.len() > MAX_ENTRY_BYTES {
                return Err(BookError::TooLarge);
            }
            crate::web::read_analysis(path, cancel)?
        }
        _ => crate::epub::read_analysis(path, cancel)?,
    };
    check_cancel(cancel)?;
    let mut budget = Budget::default();
    for chapter in &book.chapters {
        budget.accept(chapter)?;
    }
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn epub(path: &Path, chapters: &[String], picture: Option<&[u8]>) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let manifest: String = (0..chapters.len())
            .map(|i| format!("<item id='c{i}' href='c{i}.xhtml'/>"))
            .collect();
        let spine: String = (0..chapters.len())
            .map(|i| format!("<itemref idref='c{i}'/>"))
            .collect();
        for (name, text) in [
            (
                "META-INF/container.xml",
                "<container><rootfile full-path='book.opf'/></container>".to_string(),
            ),
            (
                "book.opf",
                format!(
                    "<package><metadata><title>Book</title></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"
                ),
            ),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        for (i, text) in chapters.iter().enumerate() {
            zip.start_file(format!("c{i}.xhtml"), options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        if let Some(picture) = picture {
            zip.start_file("picture.png", options).unwrap();
            zip.write_all(picture).unwrap();
        }
        zip.finish().unwrap();
    }

    // T-KNW-016: the same clean text/ruby, with no loaded image data.
    #[test]
    fn analysis_keeps_all_chapter_text_without_images() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        let picture = vec![42; 5 << 20];
        epub(
            &path,
            &[
                "<p><ruby>日本語<rt>にほんご</rt></ruby>を読む</p><img src='picture.png'/>".into(),
                "<p>食べる</p>".into(),
            ],
            Some(&picture),
        );
        let normal = crate::read(&path).unwrap();
        let text = read_text(&path, &Cancel::new()).unwrap();
        assert_eq!(text.chars(), normal.chars());
        assert_eq!(text.title, normal.title);
        assert_eq!(text.chapters.len(), 2);
        assert_eq!(
            text.chapters[0].paragraphs[0],
            normal.chapters[0].paragraphs[0]
        );
        assert_eq!(normal.images["picture.png"], picture);
        assert!(text.images.is_empty());
        assert!(
            text.chapters
                .iter()
                .flat_map(|c| &c.paragraphs)
                .all(|p| p.image.is_none())
        );
    }

    // T-KNW-016: no partial result for entry/spine/text/paragraph overflows.
    #[test]
    fn analysis_limits_fail_as_a_whole_while_normal_reading_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        for chapters in [
            vec![format!("<p>{}</p>", "a".repeat(MAX_PARAGRAPH_BYTES + 1))],
            vec!["<p>猫</p>".repeat(MAX_PARAGRAPHS + 1)],
            vec![
                format!("<p>{}</p>", "a".repeat(MAX_PARAGRAPH_BYTES))
                    .repeat(MAX_TEXT_BYTES / MAX_PARAGRAPH_BYTES + 1),
            ],
            vec!["<p>猫</p>".into(); MAX_CHAPTERS + 1],
            vec![format!(
                "<!--{}--><p>猫</p>",
                "a".repeat(MAX_ENTRY_BYTES as usize)
            )],
            vec![
                format!(
                    "<!--{}--><p>猫</p>",
                    "a".repeat(MAX_ENTRY_BYTES as usize - 64)
                );
                MAX_SOURCE_BYTES as usize / MAX_ENTRY_BYTES as usize + 1
            ],
        ] {
            epub(&path, &chapters, None);
            assert_eq!(read_text(&path, &Cancel::new()), Err(BookError::TooLarge));
            assert!(crate::read(&path).is_ok());
        }
    }

    // T-KNW-016: no manga reads, remote fetches, or work after cancellation.
    #[test]
    fn cached_web_text_manga_and_cancelled_reads_have_bounded_outcomes() {
        let dir = tempfile::tempdir().unwrap();
        let html = dir.path().join("page.html");
        std::fs::write(&html, crate::web::saved("https://example.invalid/page", "<title>Page</title><main><p>日本語</p></main><img src='https://example.invalid/image'/>")).unwrap();
        let book = read_text(&html, &Cancel::new()).unwrap();
        assert_eq!(book.chapters[0].paragraphs[0].text, "日本語");
        assert!(book.images.is_empty());
        assert_eq!(book, crate::read(&html).unwrap());
        let cbz = dir.path().join("manga.cbz");
        std::fs::write(&cbz, b"not an archive").unwrap();
        assert_eq!(read_text(&cbz, &Cancel::new()), Err(BookError::NoText));
        assert_eq!(
            read_text(dir.path(), &Cancel::new()),
            Err(BookError::NoText)
        );
        let cancel = Cancel::new();
        cancel.cancel();
        assert!(
            matches!(read_text(&html, &cancel), Err(BookError::Io(reason)) if reason == "cancelled")
        );
        let large = dir.path().join("large.epub");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert_eq!(read_text(&large, &Cancel::new()), Err(BookError::TooLarge));
        std::fs::File::create(&html)
            .unwrap()
            .set_len(MAX_ENTRY_BYTES + 1)
            .unwrap();
        assert_eq!(read_text(&html, &Cancel::new()), Err(BookError::TooLarge));
    }

    // T-KNW-016: image-only or damaged books never produce partial counts.
    #[test]
    fn image_only_and_damaged_epubs_have_no_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        epub(
            &path,
            &["<img src='picture.png'/>".into()],
            Some(b"picture"),
        );
        assert_eq!(read_text(&path, &Cancel::new()), Err(BookError::NoText));
        std::fs::write(&path, b"not an archive").unwrap();
        assert!(matches!(
            read_text(&path, &Cancel::new()),
            Err(BookError::NotEpub(_))
        ));
    }
}
