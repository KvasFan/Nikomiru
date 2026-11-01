// SPDX-License-Identifier: GPL-3.0-only

//! Comic archives and folders of pictures (MNG-001), read in the natural
//! order of their names. Each page is a chapter of one picture, so a manga
//! turns its pages, keeps its place and sits in the list as a book does.
//! Limits, checked against what is read, keep a hostile file small.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::{Book, BookError, Chapter, Paragraph};

/// The most files in an archive.
const ENTRY_LIMIT: usize = 100_000;
/// The most pages.
const PAGE_LIMIT: usize = 10_000;
/// The largest page.
const PAGE_BYTES: u64 = 32 << 20;
/// The most bytes of pages kept.
const TOTAL_BYTES: u64 = 2 << 30;
/// What is taken for a page.
const PICTURES: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];

fn format(reason: &str) -> BookError {
    BookError::NotCbz(reason.to_string())
}

/// A page, not a folder, a thumbnail of the system or anything else.
fn is_page(name: &str) -> bool {
    let hidden = name
        .split('/')
        .any(|part| part.starts_with('.') || part == "__MACOSX");
    let picture = name
        .rsplit_once('.')
        .is_some_and(|(_, ext)| PICTURES.contains(&ext.to_lowercase().as_str()));
    !hidden && picture
}

/// `page2` before `page10`, whatever the zeros, then by the plain names.
fn natural(a: &str, b: &str) -> Ordering {
    fn digits(text: &[u8]) -> &[u8] {
        let end = text
            .iter()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(text.len());
        &text[..end]
    }
    fn value(number: &[u8]) -> &[u8] {
        let zeros = number.iter().take_while(|c| **c == b'0').count();
        &number[zeros..]
    }
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    while let (Some(p), Some(q)) = (x.first(), y.first()) {
        if p.is_ascii_digit() && q.is_ascii_digit() {
            let (m, n) = (digits(x), digits(y));
            let (u, v) = (value(m), value(n));
            let order = u.len().cmp(&v.len()).then_with(|| u.cmp(v));
            if order != Ordering::Equal {
                return order;
            }
            x = &x[m.len()..];
            y = &y[n.len()..];
        } else if p == q {
            x = &x[1..];
            y = &y[1..];
        } else {
            return p.cmp(q);
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

/// Reads the CBZ at `path`.
pub fn read(path: &Path) -> Result<Book, BookError> {
    let file = File::open(path).map_err(|e| BookError::Io(e.to_string()))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format(&e.to_string()))?;
    if zip.len() > ENTRY_LIMIT {
        return Err(BookError::TooLarge);
    }
    let names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|name| is_page(name))
        .map(std::borrow::Cow::into_owned)
        .collect();
    assemble(path, names, |name| {
        let entry = zip.by_name(name).map_err(|e| format(&e.to_string()))?;
        page_bytes(entry)
    })
}

/// A page, up to the limit and one byte past it.
fn page_bytes(page: impl Read) -> Result<Vec<u8>, BookError> {
    let mut bytes = Vec::new();
    page.take(PAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format(&e.to_string()))?;
    Ok(bytes)
}

/// The pictures under `root`, as paths from it; links are not followed, so a
/// folder cannot loop.
fn pictures(root: &Path) -> Result<Vec<String>, BookError> {
    let io = |e: std::io::Error| BookError::Io(e.to_string());
    let (mut names, mut folders) = (Vec::new(), vec![String::new()]);
    let mut seen = 0;
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(root.join(&folder)).map_err(io)? {
            seen += 1;
            if seen > ENTRY_LIMIT {
                return Err(BookError::TooLarge);
            }
            let entry = entry.map_err(io)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if folder.is_empty() {
                name
            } else {
                format!("{folder}/{name}")
            };
            let kind = entry.file_type().map_err(io)?;
            if kind.is_dir() {
                folders.push(path);
            } else if kind.is_file() && is_page(&path) {
                names.push(path);
            }
        }
    }
    Ok(names)
}

/// Reads the folder of pictures at `path`.
pub fn read_folder(path: &Path) -> Result<Book, BookError> {
    let names = pictures(path)?;
    assemble(path, names, |name| {
        page_bytes(File::open(path.join(name)).map_err(|e| BookError::Io(e.to_string()))?)
    })
}

/// The book of the pages `names`, each read through `open`.
fn assemble(
    path: &Path,
    mut names: Vec<String>,
    mut open: impl FnMut(&str) -> Result<Vec<u8>, BookError>,
) -> Result<Book, BookError> {
    if names.len() > PAGE_LIMIT {
        return Err(BookError::TooLarge);
    }
    if names.is_empty() {
        return Err(BookError::NoPages);
    }
    names.sort_by(|a, b| natural(a, b));
    let mut images = BTreeMap::new();
    let mut total = 0u64;
    let mut chapters = Vec::with_capacity(names.len());
    for name in names {
        let bytes = open(&name)?;
        total += bytes.len() as u64;
        if bytes.len() as u64 > PAGE_BYTES || total > TOTAL_BYTES {
            return Err(BookError::TooLarge);
        }
        images.insert(name.clone(), bytes);
        chapters.push(Chapter {
            title: None,
            paragraphs: vec![Paragraph {
                image: Some(name),
                ..Paragraph::default()
            }],
        });
    }
    // a folder's name has no extension to leave out
    let title = if path.is_dir() {
        path.file_name()
    } else {
        path.file_stem()
    };
    let title = title
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Book {
        title,
        right_to_left: true,
        manga: true,
        chapters,
        images,
        ..Book::default()
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn archive(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    // T-MNG-001: pages in their order, whatever else the archive holds
    #[test]
    fn pages_go_in_the_order_of_their_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("第1巻.cbz");
        archive(
            &path,
            &[
                ("vol/p10.jpg", b"ten"),
                ("vol/p2.PNG", b"two"),
                ("vol/p001.webp", b"one"),
                ("vol/notes.txt", b"no"),
                ("__MACOSX/vol/._p1.jpg", b"no"),
                (".hidden/p0.jpg", b"no"),
            ],
        );
        let book = read(&path).unwrap();
        let pages: Vec<_> = book
            .chapters
            .iter()
            .map(|c| c.paragraphs[0].image.clone().unwrap())
            .collect();
        assert_eq!(pages, ["vol/p001.webp", "vol/p2.PNG", "vol/p10.jpg"]);
        assert_eq!(book.images["vol/p10.jpg"], b"ten");
        assert_eq!(book.title, "第1巻");
        assert!(book.manga && book.right_to_left);
        assert_eq!(book.chars(), 3, "a position counts pages");
        assert_eq!(book.chars_before(2, 0), 2);
    }

    // T-MNG-001: a folder reads like an archive, its sub-folders included
    #[test]
    fn a_folder_of_pictures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("第2巻");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("p10.jpg"), b"ten").unwrap();
        std::fs::write(root.join("p2.jpg"), b"two").unwrap();
        std::fs::write(root.join("sub/p3.png"), b"three").unwrap();
        std::fs::write(root.join("notes.txt"), b"no").unwrap();
        std::fs::write(root.join(".p0.jpg"), b"no").unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let book = crate::read(&root).unwrap();
        let pages: Vec<_> = book
            .chapters
            .iter()
            .map(|c| c.paragraphs[0].image.clone().unwrap())
            .collect();
        assert_eq!(pages, ["p2.jpg", "p10.jpg", "sub/p3.png"]);
        assert_eq!(book.images["sub/p3.png"], b"three");
        assert_eq!(book.title, "第2巻");
        assert!(book.manga);
        let empty = dir.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert_eq!(crate::read(&empty), Err(BookError::NoPages));
    }

    // T-MNG-001
    #[test]
    fn numbers_compare_as_numbers() {
        assert_eq!(natural("a2", "a10"), Ordering::Less);
        assert_eq!(natural("a10", "a9"), Ordering::Greater);
        assert_eq!(natural("a02", "a2b"), Ordering::Less);
        assert_eq!(natural("a", "a"), Ordering::Equal);
        assert_ne!(natural("a01", "a1"), Ordering::Equal, "still two names");
        assert_eq!(natural("v1/p9", "v2/p1"), Ordering::Less);
    }

    // T-MNG-001, T-ERR-001: what is not a manga says so
    #[test]
    fn what_is_not_a_manga() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.cbz");
        archive(&empty, &[("notes.txt", b"no")]);
        assert_eq!(read(&empty), Err(BookError::NoPages));
        let junk = dir.path().join("junk.cbz");
        std::fs::write(&junk, b"not a zip").unwrap();
        assert!(matches!(read(&junk), Err(BookError::NotCbz(_))));
    }

    // T-MNG-001: a page past the limit, however small it claims to be
    #[test]
    fn a_huge_page_stops_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.cbz");
        let page = vec![0u8; PAGE_BYTES as usize + 1];
        archive(&path, &[("p1.jpg", &page)]);
        assert_eq!(read(&path), Err(BookError::TooLarge));
    }
}
