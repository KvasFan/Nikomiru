// SPDX-License-Identifier: GPL-3.0-only

//! DSL (Lingvo): a text file, usually UTF-16, with `#NAME` and the like at
//! its top, then cards: one or more headword lines from the first column,
//! and the definition in lines that start with a blank.

use std::path::Path;

use nikomiru_core::Cancel;

use super::{Terms, bad, gunzip, index, read_file, revision_of, stem, tidy};
use crate::import::ImportError;
use crate::yomitan::{Index, Term};

fn utf16(bytes: &[u8], big: bool) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|b| {
        if big {
            u16::from_be_bytes(*b)
        } else {
            u16::from_le_bytes(*b)
        }
    });
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// The text of a file: by its byte order mark, or UTF-16 if its first bytes
/// look like it, or UTF-8.
fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xff, 0xfe, rest @ ..] => utf16(rest, false),
        [0xfe, 0xff, rest @ ..] => utf16(rest, true),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [_, 0, ..] => utf16(bytes, false),
        [0, _, ..] => utf16(bytes, true),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn text_of(path: &Path) -> Result<String, ImportError> {
    let bytes = read_file(path)?;
    let packed = path.to_string_lossy().to_lowercase().ends_with(".dz");
    let bytes = if packed { gunzip(path, &bytes)? } else { bytes };
    Ok(decode(&bytes))
}

fn name_in(text: &str) -> Option<String> {
    let line = text
        .lines()
        .take_while(|l| l.starts_with('#'))
        .find(|l| l.starts_with("#NAME"))?;
    let name = line.trim_start_matches("#NAME").trim().trim_matches('"');
    (!name.is_empty()).then(|| name.to_string())
}

pub(super) fn read_index(path: &Path) -> Result<Index, ImportError> {
    let text = text_of(path)?;
    if !text.starts_with('#') {
        return Err(bad(path, "not a DSL file"));
    }
    let title = name_in(&text).unwrap_or_else(|| stem(path));
    Ok(index(title, revision_of(path), None))
}

/// A headword: `{…}` holds what is not part of it, `\` escapes.
fn clean_head(line: &str) -> String {
    let mut out = String::new();
    let mut hidden = 0usize;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next().filter(|_| hidden == 0) {
                    out.push(next);
                }
            }
            '{' => hidden += 1,
            '}' => hidden = hidden.saturating_sub(1),
            _ if hidden == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// A definition's line: `[…]` tags and `{{…}}` comments go, `<<word>>` is the
/// word, `\` escapes.
fn clean_body(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '[' => {
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                }
            }
            '{' if chars.peek() == Some(&'{') => {
                let mut last = ' ';
                for inner in chars.by_ref() {
                    if inner == '}' && last == '}' {
                        break;
                    }
                    last = inner;
                }
            }
            '<' if chars.peek() == Some(&'<') => {
                chars.next();
            }
            '>' if chars.peek() == Some(&'>') => {
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

pub(super) fn read_terms(path: &Path, cancel: &Cancel) -> Result<Vec<Term>, ImportError> {
    let text = text_of(path)?;
    let mut terms = Terms::new(cancel);
    let (mut heads, mut body): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let mut flush = |heads: &mut Vec<String>, body: &mut Vec<String>| {
        let definition = tidy(&body.join("\n"));
        for head in heads.iter() {
            terms.add(head, &definition)?;
        }
        heads.clear();
        body.clear();
        Ok::<(), ImportError>(())
    };
    for line in text.lines() {
        if line.starts_with('#') && heads.is_empty() {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with([' ', '\t']) {
            body.push(clean_body(line));
        } else {
            if !body.is_empty() {
                flush(&mut heads, &mut body)?;
            }
            heads.push(clean_head(line));
        }
    }
    flush(&mut heads, &mut body)?;
    drop(flush);
    Ok(terms.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "#NAME \"Mini DSL\"\n#INDEX_LANGUAGE \"Japanese\"\n\n\
        食べる\nたべる【食べる】\n\t[m1][b]to eat[/b][/m] {{note}}\n\t[m2]see <<飲む>>[/m]\n\n\
        飲{む}\n\t\\[to\\] drink\n";

    fn utf16_file(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    // T-DIC-004
    #[test]
    fn a_dsl_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.dsl");
        std::fs::write(&path, utf16_file(CARD)).unwrap();
        assert_eq!(read_index(&path).unwrap().title, "Mini DSL");
        let terms = read_terms(&path, &Cancel::new()).unwrap();
        let words: Vec<_> = terms
            .iter()
            .map(|t| (t.expression.as_str(), t.reading.as_str()))
            .collect();
        assert_eq!(words, [("食べる", ""), ("食べる", "たべる"), ("飲", "")]);
        assert_eq!(terms[0].glossary, r#"["to eat\nsee 飲む"]"#);
        assert_eq!(terms[2].glossary, r#"["[to] drink"]"#);
    }

    // T-DIC-004: the three encodings, and the packed form
    #[test]
    fn encodings() {
        assert_eq!(decode(&utf16_file("#a")), "#a");
        assert_eq!(decode(b"#\0a\0"), "#a");
        assert_eq!(decode(b"\xef\xbb\xbf#a"), "#a");
        assert_eq!(decode("#あ".as_bytes()), "#あ");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.dsl.dz");
        let mut packed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut packed, CARD.as_bytes()).unwrap();
        std::fs::write(&path, packed.finish().unwrap()).unwrap();
        assert_eq!(read_terms(&path, &Cancel::new()).unwrap().len(), 3);
    }

    #[test]
    fn something_else_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.dsl");
        std::fs::write(&path, "no header").unwrap();
        assert!(read_index(&path).is_err());
    }
}
