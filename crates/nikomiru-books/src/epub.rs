// SPDX-License-Identifier: GPL-3.0-only

//! The EPUB container (BOK-001): `META-INF/container.xml` names the package,
//! whose manifest and spine name the chapters. Limits, checked against what
//! is read rather than what the archive claims, keep a hostile file small.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::markup::{chapter_of, decode};
use crate::{Book, BookError};

/// The most files in a book.
const ENTRY_LIMIT: usize = 100_000;
/// The largest text file read.
const FILE_LIMIT: u64 = 64 << 20;
/// The most text read in all.
const TOTAL_LIMIT: u64 = 1 << 30;
/// The most chapters.
const CHAPTER_LIMIT: usize = 20_000;
/// The largest picture kept.
const IMAGE_LIMIT: u64 = 32 << 20;
/// The most bytes of pictures kept.
const IMAGES_LIMIT: u64 = 512 << 20;

fn format(reason: &str) -> BookError {
    BookError::NotEpub(reason.to_string())
}

/// The attributes of the tags called `name` in `xml`, with or without a
/// namespace, each as a list of pairs.
fn tags(xml: &str, name: &str) -> Vec<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        let head = tag.split_whitespace().next().unwrap_or_default();
        if head.trim_end_matches('/').rsplit(':').next() != Some(name) {
            continue;
        }
        let mut attributes = Vec::new();
        let mut body = tag[head.len()..].trim_start();
        while let Some(eq) = body.find('=') {
            let key = body[..eq].trim().to_string();
            let after = body[eq + 1..].trim_start();
            let Some(quote) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
                break;
            };
            let Some(close) = after[1..].find(quote) else {
                break;
            };
            attributes.push((key, decode(&after[1..=close])));
            body = after[close + 2..].trim_start();
        }
        out.push(attributes);
    }
    out
}

fn attribute<'a>(tag: &'a [(String, String)], name: &str) -> Option<&'a str> {
    tag.iter()
        .find(|(k, _)| k.rsplit(':').next() == Some(name))
        .map(|(_, v)| v.as_str())
}

/// The text of the first tag called `name`.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = [format!("<dc:{name}"), format!("<{name}")]
        .iter()
        .find_map(|t| xml.find(t.as_str()).map(|at| at + t.len()))?;
    let rest = &xml[open..];
    let body = &rest[rest.find('>')? + 1..];
    Some(decode(body[..body.find('<')?].trim()))
}

/// A path of the archive as the package writes it: percent escapes read,
/// relative to the package's folder, `.` and `..` resolved. `None` if it
/// climbs out.
fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.split('#').next()?;
    let mut bytes = Vec::with_capacity(href.len());
    let raw = href.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        let hex = (raw[i] == b'%')
            .then(|| href.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) => {
                bytes.push(byte);
                i += 3;
            }
            None => {
                bytes.push(raw[i]);
                i += 1;
            }
        }
    }
    let href = String::from_utf8_lossy(&bytes);
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or(Vec::new(), |(dir, _)| dir.split('/').collect())
    };
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

struct Archive {
    zip: ZipArchive<File>,
    left: u64,
    /// The bytes of pictures still allowed.
    pictures: u64,
}

impl Archive {
    /// A picture, or nothing when it cannot be had: the book reads without.
    fn image(&mut self, name: &str) -> Option<Vec<u8>> {
        let entry = self.zip.by_name(name).ok()?;
        let limit = IMAGE_LIMIT.min(self.pictures);
        let mut bytes = Vec::new();
        entry.take(limit + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > limit {
            return None;
        }
        self.pictures -= bytes.len() as u64;
        Some(bytes)
    }

    fn text(&mut self, name: &str) -> Result<String, BookError> {
        let entry = self
            .zip
            .by_name(name)
            .map_err(|_| format("a file of the book is missing"))?;
        let limit = FILE_LIMIT.min(self.left);
        let mut bytes = Vec::new();
        entry
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| BookError::NotEpub(e.to_string()))?;
        if bytes.len() as u64 > limit {
            return Err(BookError::TooLarge);
        }
        self.left -= bytes.len() as u64;
        Ok(String::from_utf8_lossy(&bytes)
            .trim_start_matches('\u{feff}')
            .to_string())
    }
}

/// Reads the book at `path`.
pub fn read(path: &Path) -> Result<Book, BookError> {
    let file = File::open(path).map_err(|e| BookError::Io(e.to_string()))?;
    let zip = ZipArchive::new(file).map_err(|_| format("not a zip archive"))?;
    if zip.len() > ENTRY_LIMIT {
        return Err(BookError::TooLarge);
    }
    let mut archive = Archive {
        zip,
        left: TOTAL_LIMIT,
        pictures: IMAGES_LIMIT,
    };
    let container = archive.text("META-INF/container.xml")?;
    let package = tags(&container, "rootfile")
        .iter()
        .find_map(|t| attribute(t, "full-path").map(str::to_string))
        .ok_or_else(|| format("no package"))?;
    let opf = archive.text(&package)?;
    let items = tags(&opf, "item");
    let manifest: HashMap<&str, &str> = items
        .iter()
        .filter_map(|t| Some((attribute(t, "id")?, attribute(t, "href")?)))
        .collect();
    let spine = tags(&opf, "itemref");
    if spine.len() > CHAPTER_LIMIT {
        return Err(BookError::TooLarge);
    }
    let mut chapters = Vec::new();
    let mut images = std::collections::BTreeMap::new();
    for itemref in &spine {
        let Some(href) = attribute(itemref, "idref").and_then(|id| manifest.get(id)) else {
            continue;
        };
        // what climbs out of the archive is not a chapter
        let Some(name) = resolve(&package, href) else {
            continue;
        };
        let mut chapter = chapter_of(&archive.text(&name)?);
        // pictures are relative to their chapter, and one that cannot be had
        // is left out
        chapter.paragraphs.retain_mut(|paragraph| {
            let Some(source) = paragraph.image.take() else {
                return true;
            };
            let Some(key) = resolve(&name, &source) else {
                return false;
            };
            if !images.contains_key(&key) {
                match archive.image(&key) {
                    Some(bytes) => {
                        images.insert(key.clone(), bytes);
                    }
                    None => return false,
                }
            }
            paragraph.image = Some(key);
            true
        });
        if !chapter.paragraphs.is_empty() {
            chapters.push(chapter);
        }
    }
    let has_text = chapters
        .iter()
        .flat_map(|c| &c.paragraphs)
        .any(|p| !p.text.is_empty());
    if !has_text {
        return Err(BookError::NoText);
    }
    let stem = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let right_to_left = tags(&opf, "spine")
        .iter()
        .any(|t| attribute(t, "page-progression-direction") == Some("rtl"));
    Ok(Book {
        right_to_left,
        title: element(&opf, "title")
            .filter(|t| !t.is_empty())
            .unwrap_or(stem),
        author: element(&opf, "creator").unwrap_or_default(),
        chapters,
        images,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn epub(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, text) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    const CONTAINER: &str = r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    const OPF: &str = r#"<package><metadata><dc:title>吾輩は猫である</dc:title><dc:creator id="a">夏目漱石</dc:creator></metadata>
        <manifest><item id="c1" href="text/one%20a.xhtml" media-type="application/xhtml+xml"/><item href='../two.xhtml' id='c2'/><item id="c3" href="../../evil.xhtml"/><item id="pic" href="cover.jpg"/></manifest>
        <spine toc="ncx"><itemref idref="c2"/><itemref idref="c1"/><itemref idref="c3"/><itemref idref="none"/></spine></package>"#;

    fn sample(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("book.epub");
        epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/text/one a.xhtml", "<p>第二</p>"),
                ("two.xhtml", "<h1>序</h1><p>第一</p>"),
            ],
        );
        path
    }

    // T-BOK-LIVE-001: a real book, from `NIKOMIRU_REAL_EPUB`
    #[test]
    #[ignore = "L7: needs a real EPUB"]
    fn l7_a_real_epub_is_read() {
        let Ok(path) = std::env::var("NIKOMIRU_REAL_EPUB") else {
            eprintln!("skipped: no NIKOMIRU_REAL_EPUB");
            return;
        };
        let book = read(Path::new(&path)).unwrap();
        eprintln!(
            "{} — {}: {} chapters, {} characters",
            book.title,
            book.author,
            book.chapters.len(),
            book.chars()
        );
        for chapter in book.chapters.iter().take(6) {
            let first = chapter
                .paragraphs
                .first()
                .map(|p| p.text.chars().take(50).collect::<String>());
            let ruby: usize = chapter.paragraphs.iter().map(|p| p.ruby.len()).sum();
            eprintln!(
                "  {:?}: {} paragraphs, {ruby} readings, {first:?}",
                chapter.title,
                chapter.paragraphs.len()
            );
        }
        assert!(book.chars() > 1000);
    }

    // T-BOK-001: the spine's order, paths relative to the package
    #[test]
    fn a_book_is_read_in_its_order() {
        let dir = tempfile::tempdir().unwrap();
        let book = read(&sample(dir.path())).unwrap();
        assert_eq!(book.title, "吾輩は猫である");
        assert_eq!(book.author, "夏目漱石");
        assert_eq!(book.chapters.len(), 2);
        assert_eq!(book.chapters[0].title.as_deref(), Some("序"));
        assert_eq!(book.chapters[1].paragraphs[0].text, "第二");
        assert!(!book.right_to_left);
    }

    // T-BOK-001
    #[test]
    fn paths_stay_in_the_archive() {
        assert_eq!(
            resolve("a/b.opf", "c/d.xhtml#x").as_deref(),
            Some("a/c/d.xhtml")
        );
        assert_eq!(resolve("a/b.opf", "../d.xhtml").as_deref(), Some("d.xhtml"));
        assert_eq!(
            resolve("a/b.opf", "x%20y.xhtml").as_deref(),
            Some("a/x y.xhtml")
        );
        assert_eq!(resolve("b.opf", "../d.xhtml"), None);
    }

    // T-BOK-010: a picture is kept by its place, one that is not there is
    // left out, and pictures alone are not a book
    #[test]
    fn pictures_come_with_the_book() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                (
                    "OEBPS/text/one a.xhtml",
                    "<p>本文</p><img src='../images/a.png'/><img src='gone.png'/>",
                ),
                ("OEBPS/images/a.png", "bytes"),
                ("two.xhtml", "<p>序</p>"),
            ],
        );
        let book = read(&path).unwrap();
        assert_eq!(book.images["OEBPS/images/a.png"], b"bytes");
        let chapter = &book.chapters[1];
        assert_eq!(chapter.paragraphs.len(), 2, "{chapter:?}");
        assert_eq!(
            chapter.paragraphs[1].image.as_deref(),
            Some("OEBPS/images/a.png")
        );
        epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("OEBPS/text/one a.xhtml", "<img src='a.png'/>"),
                ("OEBPS/text/a.png", "bytes"),
                ("two.xhtml", ""),
            ],
        );
        assert_eq!(read(&path), Err(BookError::NoText));
    }

    // T-BOK-009
    #[test]
    fn a_book_says_it_reads_from_the_right() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        let opf = OPF.replace("<spine ", r#"<spine page-progression-direction="rtl" "#);
        epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &opf),
                ("OEBPS/text/one a.xhtml", "<p>本文</p>"),
                ("two.xhtml", "<p>本文</p>"),
            ],
        );
        assert!(read(&path).unwrap().right_to_left);
    }

    // T-BOK-001: what is not a book says so
    #[test]
    fn broken_books_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.epub");
        std::fs::write(&path, "not a zip").unwrap();
        assert!(matches!(read(&path), Err(BookError::NotEpub(_))));
        epub(&path, &[("other.txt", "x")]);
        assert!(matches!(read(&path), Err(BookError::NotEpub(_))));
        epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", OPF),
                ("two.xhtml", "<p><img src='a.png'/></p>"),
                ("OEBPS/text/one a.xhtml", ""),
            ],
        );
        assert_eq!(read(&path), Err(BookError::NoText));
        assert!(matches!(
            read(&dir.path().join("missing.epub")),
            Err(BookError::Io(_))
        ));
    }

    #[test]
    fn attributes_in_either_quote() {
        let t = &tags("<a x='1' y=\"2&amp;\"/>", "a")[0];
        assert_eq!(attribute(t, "x"), Some("1"));
        assert_eq!(attribute(t, "y"), Some("2&"));
        assert!(tags("<a x=1>", "a")[0].is_empty());
    }
}
