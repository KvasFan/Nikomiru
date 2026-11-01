// SPDX-License-Identifier: GPL-3.0-only

//! StarDict: `.ifo` with the metadata, `.idx` (or `.idx.gz`) with each word,
//! offset and size, and `.dict` (or `.dict.dz`, which gzip reads) with the
//! definitions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nikomiru_core::Cancel;

use super::{Terms, bad, gunzip, html_to_text, index, read_file, revision_of, stem};
use crate::import::ImportError;
use crate::yomitan::{Index, Term};

fn metadata(path: &Path) -> Result<HashMap<String, String>, ImportError> {
    let bytes = read_file(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if !lines
        .next()
        .is_some_and(|l| l.trim_start_matches('\u{feff}').starts_with("StarDict"))
    {
        return Err(bad(path, "not a StarDict .ifo"));
    }
    Ok(lines
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect())
}

pub(super) fn read_index(path: &Path) -> Result<Index, ImportError> {
    let meta = metadata(path)?;
    let title = meta
        .get("bookname")
        .filter(|t| !t.is_empty())
        .cloned()
        .unwrap_or_else(|| stem(path));
    let revision = meta
        .get("date")
        .cloned()
        .unwrap_or_else(|| revision_of(path));
    // real ones hold HTML here
    let description = meta.get("description").map(|d| html_to_text(d));
    Ok(index(title, revision, description))
}

/// `base` with one of `endings`, the first that exists.
fn sibling(path: &Path, endings: &[&str]) -> Option<(PathBuf, bool)> {
    endings.iter().find_map(|ending| {
        let candidate = path.with_extension(ending);
        candidate
            .exists()
            .then(|| (candidate, ending.ends_with("gz") || ending.ends_with("dz")))
    })
}

fn open(path: &Path, endings: &[&str], what: &str) -> Result<Vec<u8>, ImportError> {
    let Some((file, packed)) = sibling(path, endings) else {
        return Err(bad(path, format!("its {what} file is missing")));
    };
    let bytes = read_file(&file)?;
    if packed {
        gunzip(&file, &bytes)
    } else {
        Ok(bytes)
    }
}

/// The text of a type of data, if it has any.
fn text_of(kind: u8, data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    match kind {
        b'm' | b'l' | b'y' | b't' => Some(text.into_owned()),
        b'h' | b'g' | b'x' | b'k' | b'w' => Some(html_to_text(&text)),
        _ => None,
    }
}

/// The parts of a record whose types are `types`, in order; the last one takes
/// what is left. Lower-case types end at a NUL, upper-case ones have a size.
fn with_types(mut data: &[u8], types: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, kind) in types.iter().enumerate() {
        let last = i + 1 == types.len();
        let (part, rest): (&[u8], &[u8]) = if kind.is_ascii_uppercase() {
            if last {
                (data, &[])
            } else {
                let Some((size, tail)) = data.split_first_chunk::<4>() else {
                    break;
                };
                let size = (u32::from_be_bytes(*size) as usize).min(tail.len());
                tail.split_at(size)
            }
        } else if last {
            (data, &[])
        } else {
            let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
            (&data[..end], data.get(end + 1..).unwrap_or(&[]))
        };
        out.extend(text_of(*kind, part));
        data = rest;
    }
    out
}

/// The parts of a record that says its types itself.
fn typed(mut data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    while let Some((kind, rest)) = data.split_first() {
        let (part, tail): (&[u8], &[u8]) = if kind.is_ascii_uppercase() {
            let Some((size, tail)) = rest.split_first_chunk::<4>() else {
                break;
            };
            let size = (u32::from_be_bytes(*size) as usize).min(tail.len());
            tail.split_at(size)
        } else {
            let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
            (&rest[..end], rest.get(end + 1..).unwrap_or(&[]))
        };
        out.extend(text_of(*kind, part));
        data = tail;
    }
    out
}

pub(super) fn read_terms(path: &Path, cancel: &Cancel) -> Result<Vec<Term>, ImportError> {
    let meta = metadata(path)?;
    let wide = meta.get("idxoffsetbits").is_some_and(|b| b == "64");
    let types = meta
        .get("sametypesequence")
        .map(|t| t.as_bytes().to_vec())
        .unwrap_or_default();
    let idx = open(path, &["idx", "idx.gz"], "index")?;
    let dict = open(path, &["dict", "dict.dz"], "definitions")?;
    let mut terms = Terms::new(cancel);
    let mut rest = idx.as_slice();
    while !rest.is_empty() {
        let Some(end) = rest.iter().position(|b| *b == 0) else {
            return Err(bad(path, "a word has no end"));
        };
        let word = String::from_utf8_lossy(&rest[..end]);
        let after = &rest[end + 1..];
        let (offset, size, tail) = if wide {
            let Some((offset, tail)) = after.split_first_chunk::<8>() else {
                return Err(bad(path, "the index is cut short"));
            };
            let Some((size, tail)) = tail.split_first_chunk::<4>() else {
                return Err(bad(path, "the index is cut short"));
            };
            (u64::from_be_bytes(*offset), u32::from_be_bytes(*size), tail)
        } else {
            let Some((offset, tail)) = after.split_first_chunk::<4>() else {
                return Err(bad(path, "the index is cut short"));
            };
            let Some((size, tail)) = tail.split_first_chunk::<4>() else {
                return Err(bad(path, "the index is cut short"));
            };
            (
                u64::from(u32::from_be_bytes(*offset)),
                u32::from_be_bytes(*size),
                tail,
            )
        };
        rest = tail;
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let Some(data) = start
            .checked_add(size as usize)
            .and_then(|stop| dict.get(start..stop))
        else {
            return Err(bad(path, "an entry is outside the definitions"));
        };
        let parts = if types.is_empty() {
            typed(data)
        } else {
            with_types(data, &types)
        };
        terms.add(&word, &parts.join("\n"))?;
    }
    Ok(terms.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(idx: &mut Vec<u8>, word: &str, offset: u32, size: u32) {
        idx.extend_from_slice(word.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&offset.to_be_bytes());
        idx.extend_from_slice(&size.to_be_bytes());
    }

    fn write(dir: &Path, ifo: &str, idx: &[u8], dict: &[u8]) -> PathBuf {
        let path = dir.join("book.ifo");
        std::fs::write(&path, ifo).unwrap();
        std::fs::write(dir.join("book.idx"), idx).unwrap();
        std::fs::write(dir.join("book.dict"), dict).unwrap();
        path
    }

    // T-DIC-004
    #[test]
    fn a_stardict_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = Vec::new();
        entry(&mut idx, "たべる【食べる】", 0, 6);
        entry(&mut idx, "飲む", 6, 22);
        let dict = "to eat<b>to</b> <i>drink</i>";
        let ifo = "StarDict's dict ifo file\nversion=2.4.2\nbookname=Mini\nsametypesequence=h\n";
        let path = write(dir.path(), ifo, &idx, dict.as_bytes());
        assert_eq!(read_index(&path).unwrap().title, "Mini");
        let terms = read_terms(&path, &Cancel::new()).unwrap();
        assert_eq!(terms.len(), 2);
        assert_eq!(
            (terms[0].expression.as_str(), terms[0].reading.as_str()),
            ("食べる", "たべる")
        );
        assert_eq!(terms[0].glossary, r#"["to eat"]"#);
        assert_eq!(terms[1].glossary, r#"["to drink"]"#);
    }

    // T-DIC-004: records that say their types, `m` then `h`
    #[test]
    fn records_with_their_types() {
        assert_eq!(typed(b"mone\0h<b>two</b>\0"), ["one", "two"]);
        assert_eq!(with_types(b"one\0<b>two</b>", b"mh"), ["one", "two"]);
        assert_eq!(typed(b"X\0\0\0\xffbytes"), Vec::<String>::new());
    }

    // T-DIC-004: nothing in a file is believed
    #[test]
    fn broken_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = Vec::new();
        entry(&mut idx, "語", 100, 100);
        let ifo = "StarDict's dict ifo file\nbookname=Mini\n";
        let path = write(dir.path(), ifo, &idx, b"short");
        assert!(read_terms(&path, &Cancel::new()).is_err());
        std::fs::write(dir.path().join("book.idx"), b"no end").unwrap();
        assert!(read_terms(&path, &Cancel::new()).is_err());
        std::fs::remove_file(dir.path().join("book.dict")).unwrap();
        assert!(read_terms(&path, &Cancel::new()).is_err());
        std::fs::write(&path, "something else").unwrap();
        assert!(read_index(&path).is_err());
    }
}
