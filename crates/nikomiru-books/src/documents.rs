// SPDX-License-Identifier: GPL-3.0-only

//! Bounded inert HTMLZ, RTF, DOCX and text-layer PDF sources (DOC-001…005).
use crate::{Book, BookError, Chapter};
use nikomiru_core::Cancel;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
    path::Path,
};
mod docx;
mod rtf;

/// Native format operations, kept out of the pure reader and supplied by the app.
pub trait Support: Send + Sync {
    /// Update the explicit PDF helper path for subsequent operations; built-in callers need no configuration.
    fn configure_pdf(&self, _path: String) {}
    /// Decode a declared legacy code page without guessing or replacement.
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String>;
    /// Extract bounded UTF-8 text, retaining form-feed page boundaries, without OCR.
    fn pdf_text(&self, bytes: &[u8], cancel: &Cancel) -> Result<String, BookError>;
}
/// Unicode/Windows-1252 support for callers without a native application adapter.
#[derive(Debug)]
pub struct Builtin;
impl Support for Builtin {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Option<String> {
        if matches!(encoding, "UTF-8" | "CP65001") {
            return String::from_utf8(bytes.to_vec()).ok();
        }
        if encoding != "CP1252" && !bytes.is_ascii() {
            return None;
        }
        let mut out = String::new();
        for &b in bytes {
            let c = match b {
                0x80 => '€',
                0x82 => '‚',
                0x83 => 'ƒ',
                0x84 => '„',
                0x85 => '…',
                0x86 => '†',
                0x87 => '‡',
                0x88 => 'ˆ',
                0x89 => '‰',
                0x8a => 'Š',
                0x8b => '‹',
                0x8c => 'Œ',
                0x8e => 'Ž',
                0x91 => '‘',
                0x92 => '’',
                0x93 => '“',
                0x94 => '”',
                0x95 => '•',
                0x96 => '–',
                0x97 => '—',
                0x98 => '˜',
                0x99 => '™',
                0x9a => 'š',
                0x9b => '›',
                0x9c => 'œ',
                0x9e => 'ž',
                0x9f => 'Ÿ',
                0x81 | 0x8d | 0x8f | 0x90 | 0x9d => return None,
                b => char::from(b),
            };
            out.push(c);
        }
        Some(out)
    }
    fn pdf_text(&self, _: &[u8], _: &Cancel) -> Result<String, BookError> {
        Err(BookError::PdfUnavailable)
    }
}
const MAX_ARCHIVE: usize = 16 << 20;
const MAX_ENTRY: usize = 4 << 20;
const MAX_ENTRIES: usize = 2048;
const MAX_ARCHIVE_CONTENT: u64 = 64 << 20;
fn bad() -> BookError {
    BookError::InvalidDocument
}
fn read_bytes(path: &Path, max: usize, cancel: &Cancel) -> Result<Vec<u8>, BookError> {
    crate::analysis::check_cancel(cancel)?;
    let file = std::fs::File::open(path).map_err(|e| BookError::Io(e.to_string()))?;
    let meta = file.metadata().map_err(|e| BookError::Io(e.to_string()))?;
    if !meta.is_file() {
        return Err(bad());
    }
    if meta.len() > max as u64 {
        return Err(BookError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| BookError::Io(e.to_string()))?;
    if bytes.len() > max {
        return Err(BookError::TooLarge);
    }
    crate::analysis::check_cancel(cancel)?;
    Ok(bytes)
}
struct Archive<'a> {
    zip: zip::ZipArchive<Cursor<&'a [u8]>>,
    names: HashMap<String, usize>,
}
impl<'a> Archive<'a> {
    fn new(bytes: &'a [u8], cancel: &Cancel) -> Result<Self, BookError> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| bad())?;
        if zip.len() > MAX_ENTRIES {
            return Err(BookError::TooLarge);
        }
        let mut names = HashMap::new();
        let mut folded = HashSet::new();
        let mut size = 0u64;
        for i in 0..zip.len() {
            crate::analysis::check_cancel(cancel)?;
            let f = zip.by_index(i).map_err(|_| bad())?;
            let name = f.name().map_err(|_| bad())?;
            let clean = name.trim_end_matches('/');
            if clean.is_empty()
                || clean.len() > 1024
                || clean
                    .split('/')
                    .any(|c| c.is_empty() || c == "." || c == "..")
                || clean.contains(['\\', ':', '\0'])
                || clean.starts_with('/')
                || f.is_symlink()
                || !folded.insert(clean.to_lowercase())
            {
                return Err(bad());
            }
            size = size.checked_add(f.size()).ok_or(BookError::TooLarge)?;
            if size > MAX_ARCHIVE_CONTENT {
                return Err(BookError::TooLarge);
            }
            if !f.is_dir() {
                names.insert(name.into_owned(), i);
            }
        }
        Ok(Self { zip, names })
    }
    fn text(&mut self, name: &str, cancel: &Cancel) -> Result<String, BookError> {
        crate::analysis::check_cancel(cancel)?;
        let i = *self.names.get(name).ok_or_else(bad)?;
        let f = self.zip.by_index(i).map_err(|_| bad())?;
        if f.size() > MAX_ENTRY as u64 {
            return Err(BookError::TooLarge);
        }
        let mut out = Vec::new();
        f.take(MAX_ENTRY as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|_| bad())?;
        if out.len() > MAX_ENTRY {
            return Err(BookError::TooLarge);
        }
        String::from_utf8(out)
            .map(|s| s.trim_start_matches('\u{feff}').to_owned())
            .map_err(|_| bad())
    }
}
fn htmlz(bytes: &[u8], cancel: &Cancel) -> Result<Vec<Chapter>, BookError> {
    let mut a = Archive::new(bytes, cancel)?;
    let index: Vec<_> = a
        .names
        .keys()
        .filter(|n| matches!(n.to_ascii_lowercase().as_str(), "index.html" | "index.htm"))
        .cloned()
        .collect();
    let name = if index.len() == 1 {
        index[0].clone()
    } else if index.is_empty() {
        let html: Vec<_> = a
            .names
            .keys()
            .filter(|n| {
                Path::new(n).extension().is_some_and(|e| {
                    matches!(
                        e.to_string_lossy().to_ascii_lowercase().as_str(),
                        "html" | "htm" | "xhtml"
                    )
                })
            })
            .cloned()
            .collect();
        if html.len() != 1 {
            return Err(bad());
        }
        html[0].clone()
    } else {
        return Err(bad());
    };
    let mut chapter = crate::markup::chapter_of(&a.text(&name, cancel)?);
    chapter.paragraphs.retain(|p| p.image.is_none());
    Ok(vec![chapter])
}
fn finish(
    title: &str,
    format: &str,
    bytes: &[u8],
    chapters: Vec<Chapter>,
) -> Result<Book, BookError> {
    if title.trim().is_empty() || title.chars().count() > 256 || title.chars().any(char::is_control)
    {
        return Err(bad());
    }
    let mut count = 0usize;
    let mut total = 0usize;
    for p in chapters.iter().flat_map(|c| &c.paragraphs) {
        count += 1;
        total = total.saturating_add(p.text.len());
        if count > crate::text::MAX_PARAGRAPHS
            || p.text.len() > crate::text::MAX_PARAGRAPH_BYTES
            || total > crate::text::MAX_BYTES
        {
            return Err(BookError::TooLarge);
        }
        if p.text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(bad());
        }
    }
    if !chapters
        .iter()
        .flat_map(|c| &c.paragraphs)
        .any(|p| !p.text.trim().is_empty())
    {
        return Err(BookError::NoText);
    }
    let mut h = Sha256::new();
    for b in [title.as_bytes(), format.as_bytes(), bytes] {
        h.update((b.len() as u64).to_le_bytes());
        h.update(b);
    }
    // Converted paragraph boundaries are part of identity, including native helper/decoder effects.
    for chapter in &chapters {
        h.update((chapter.paragraphs.len() as u64).to_le_bytes());
        for p in &chapter.paragraphs {
            h.update((p.text.len() as u64).to_le_bytes());
            h.update(p.text.as_bytes());
        }
    }
    Ok(Book {
        identity: Some(format!(
            "document:{}",
            h.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )),
        title: title.into(),
        chapters,
        ..Default::default()
    })
}
/// Read one explicitly selected additional format, using only the supplied native support.
pub fn read(path: &Path, support: &dyn Support, cancel: &Cancel) -> Result<Book, BookError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .ok_or_else(bad)?;
    let bytes = read_bytes(
        path,
        if ext == "rtf" { MAX_ENTRY } else { MAX_ARCHIVE },
        cancel,
    )?;
    let title = path.file_stem().unwrap_or_default().to_string_lossy();
    let chapters = match ext.as_str() {
        "htmlz" => htmlz(&bytes, cancel)?,
        "docx" => docx::read(&bytes, cancel)?,
        "rtf" => rtf::read(&bytes, support, cancel)?,
        "pdf" => {
            if !bytes.starts_with(b"%PDF-") {
                return Err(bad());
            }
            let text = support.pdf_text(&bytes, cancel)?;
            if text.len() > MAX_ENTRY {
                return Err(BookError::TooLarge);
            }
            // Poppler emits a trailing page separator, not an additional empty page.
            let text = text.strip_suffix('\u{000c}').unwrap_or(&text);
            let mut chapters = Vec::new();
            for page in text.split('\u{000c}') {
                if chapters.len() >= 512 {
                    return Err(BookError::TooLarge);
                }
                chapters.push(Chapter {
                    paragraphs: page
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|line| crate::Paragraph {
                            text: line.into(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                });
            }
            chapters
        }
        _ => return Err(bad()),
    };
    crate::analysis::check_cancel(cancel)?;
    finish(&title, &ext, &bytes, chapters)
}

#[cfg(test)]
mod tests;
