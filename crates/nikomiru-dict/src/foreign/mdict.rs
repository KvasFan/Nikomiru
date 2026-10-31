// SPDX-License-Identifier: GPL-3.0-only

//! MDict `.mdx`, engine versions 1.2 and 2.0: an XML header in UTF-16, the
//! headwords in blocks, the definitions in blocks. The structure follows
//! writemdict's description of the format. Blocks compressed with zlib or not
//! at all are read; LZO and dictionaries locked to a user (`Encrypted` with
//! bit 1) are refused. Bit 2 only obscures the index, with a key made from
//! the index itself.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use nikomiru_core::Cancel;

use super::{
    Terms, UNPACKED_LIMIT, bad, html_to_text, index, read_file, revision_of, ripemd, stem,
};
use crate::import::ImportError;
use crate::yomitan::{Index, Term};

/// How many links are followed from a headword.
const LINK_DEPTH: usize = 4;

/// A cursor over bytes: every read is checked against what is there.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: u64) -> Option<&'a [u8]> {
        let count = usize::try_from(count).ok()?;
        let end = self.at.checked_add(count)?;
        let part = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(part)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    /// A number as wide as the format's.
    fn number(&mut self, wide: bool) -> Option<u64> {
        if wide {
            self.u64()
        } else {
            self.u32().map(u64::from)
        }
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for byte in chunk {
            a += u32::from(*byte);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

/// Undoes the index's obscuring.
fn unscramble(data: &mut [u8], key: &[u8; 16]) {
    let mut previous = 0x36u8;
    for (i, byte) in data.iter_mut().enumerate() {
        let original = *byte;
        *byte = original.rotate_left(4) ^ previous ^ (i as u8) ^ key[i % key.len()];
        previous = original;
    }
}

/// The attributes of the XML header.
#[derive(Debug, Default, PartialEq)]
struct Header {
    title: String,
    description: String,
    version: f32,
    encrypted: u32,
    utf16: bool,
}

fn attribute(xml: &str, name: &str) -> Option<String> {
    let at = xml.find(&format!(" {name}=\""))? + name.len() + 3;
    let rest = &xml[at..];
    Some(rest[..rest.find('"')?].to_string())
}

fn header_of(path: &Path, text: &[u8]) -> Result<Header, ImportError> {
    let units = text
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b));
    let xml: String = char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if !xml.contains("<Dictionary") {
        return Err(bad(path, "not an MDict file"));
    }
    let encoding = attribute(&xml, "Encoding").unwrap_or_default();
    let utf16 = match encoding.to_ascii_uppercase().as_str() {
        "" | "UTF-8" | "UTF8" => false,
        "UTF-16" | "UTF16" => true,
        other => return Err(bad(path, format!("the encoding {other} is not read"))),
    };
    let encrypted = attribute(&xml, "Encrypted").unwrap_or_default();
    Ok(Header {
        title: attribute(&xml, "Title").unwrap_or_default(),
        description: attribute(&xml, "Description").unwrap_or_default(),
        version: attribute(&xml, "GeneratedByEngineVersion")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0),
        encrypted: match encrypted.as_str() {
            "Yes" => 1,
            "" | "No" => 0,
            number => number.parse().unwrap_or(0),
        },
        utf16,
    })
}

/// The header, and the cursor after it.
fn open<'a>(path: &Path, bytes: &'a [u8]) -> Result<(Header, Cursor<'a>), ImportError> {
    let cut = || bad(path, "cut short");
    let mut cursor = Cursor::new(bytes);
    let size = cursor.u32().ok_or_else(cut)?;
    let text = cursor.take(u64::from(size)).ok_or_else(cut)?;
    let sum = cursor.take(4).ok_or_else(cut)?;
    if sum != adler32(text).to_le_bytes() {
        return Err(bad(path, "the header is damaged"));
    }
    Ok((header_of(path, text)?, cursor))
}

pub(super) fn read_index(path: &Path) -> Result<Index, ImportError> {
    let bytes = read_file(path)?;
    let (header, _) = open(path, &bytes)?;
    let title = if header.title.trim().is_empty() || header.title == "Title (No HTML code allowed)"
    {
        stem(path)
    } else {
        header.title
    };
    let description = (!header.description.is_empty()).then(|| html_to_text(&header.description));
    Ok(index(title, revision_of(path), description))
}

fn inflate(path: &Path, data: &[u8], size: u64) -> Result<Vec<u8>, ImportError> {
    if size > UNPACKED_LIMIT {
        return Err(bad(path, "a block is too large"));
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take(size + 1)
        .read_to_end(&mut out)
        .map_err(|e| bad(path, e.to_string()))?;
    if out.len() as u64 != size {
        return Err(bad(path, "a block is not the size it says"));
    }
    Ok(out)
}

/// A block of headwords or definitions.
fn unpack(path: &Path, block: &[u8], size: u64) -> Result<Vec<u8>, ImportError> {
    let cut = || bad(path, "cut short");
    let mut cursor = Cursor::new(block);
    let kind = cursor.take(4).ok_or_else(cut)?;
    let sum = cursor.u32().ok_or_else(cut)?;
    let data = &block[8..];
    let out = match kind {
        [0, 0, 0, 0] => data.to_vec(),
        [2, 0, 0, 0] => inflate(path, data, size)?,
        [1, 0, 0, 0] => return Err(bad(path, "LZO blocks are not read")),
        _ => return Err(bad(path, "a block of an unknown kind")),
    };
    if adler32(&out) != sum {
        return Err(bad(path, "a block is damaged"));
    }
    Ok(out)
}

/// A text of the dictionary's encoding.
fn text_of(bytes: &[u8], utf16: bool) -> String {
    if utf16 {
        let units = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b));
        char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// The sizes of the blocks of headwords, from the block index: compressed and
/// unpacked.
fn key_block_sizes(
    path: &Path,
    info: &[u8],
    blocks: u64,
    wide: bool,
    utf16: bool,
) -> Result<Vec<(u64, u64)>, ImportError> {
    let cut = || bad(path, "the block index is cut short");
    let mut cursor = Cursor::new(info);
    let unit = if utf16 { 2 } else { 1 };
    let terminator = if wide { unit } else { 0 };
    let mut out = Vec::new();
    for _ in 0..blocks {
        cursor.number(wide).ok_or_else(cut)?;
        for _ in 0..2 {
            let chars = if wide {
                u64::from(cursor.u16().ok_or_else(cut)?)
            } else {
                u64::from(cursor.u8().ok_or_else(cut)?)
            };
            cursor.take(chars * unit + terminator).ok_or_else(cut)?;
        }
        let compressed = cursor.number(wide).ok_or_else(cut)?;
        let unpacked = cursor.number(wide).ok_or_else(cut)?;
        out.push((compressed, unpacked));
    }
    Ok(out)
}

/// The headwords of an unpacked block: each with where its record starts.
fn keys_in(
    path: &Path,
    block: &[u8],
    wide: bool,
    utf16: bool,
    out: &mut Vec<(u64, String)>,
) -> Result<(), ImportError> {
    let cut = || bad(path, "a block of headwords is cut short");
    let mut cursor = Cursor::new(block);
    while !cursor.done() {
        let start = cursor.number(wide).ok_or_else(cut)?;
        let rest = &block[cursor.at..];
        let end = if utf16 {
            rest.as_chunks::<2>()
                .0
                .iter()
                .position(|b| *b == [0, 0])
                .map(|i| i * 2)
        } else {
            rest.iter().position(|b| *b == 0)
        }
        .ok_or_else(cut)?;
        out.push((start, text_of(&rest[..end], utf16)));
        cursor.take(end as u64 + if utf16 { 2 } else { 1 });
    }
    Ok(())
}

/// The headwords, each with where its record starts, in order, and the cursor
/// after them.
fn read_keys<'a>(
    path: &Path,
    header: &Header,
    mut cursor: Cursor<'a>,
) -> Result<(Vec<(u64, String)>, Cursor<'a>), ImportError> {
    let cut = || bad(path, "the headwords are cut short");
    if header.encrypted & 1 != 0 {
        return Err(bad(path, "it is locked to a user"));
    }
    let wide = header.version >= 2.0;
    let before = cursor.at;
    let blocks = cursor.number(wide).ok_or_else(cut)?;
    let _entries = cursor.number(wide).ok_or_else(cut)?;
    if wide {
        cursor.u64().ok_or_else(cut)?;
    }
    let info_size = cursor.number(wide).ok_or_else(cut)?;
    let keys_size = cursor.number(wide).ok_or_else(cut)?;
    if wide {
        let counted = adler32(&cursor.bytes[before..cursor.at]);
        if cursor.u32().ok_or_else(cut)? != counted {
            return Err(bad(path, "the headwords' header is damaged"));
        }
    }
    let info = cursor.take(info_size).ok_or_else(cut)?;
    let info = if wide {
        if info.len() < 8 || info[..4] != [2, 0, 0, 0] {
            return Err(bad(path, "the block index is not in a known form"));
        }
        let mut data = info[8..].to_vec();
        if header.encrypted & 2 != 0 {
            let mut seed = info[4..8].to_vec();
            seed.extend_from_slice(&0x3695u32.to_le_bytes());
            unscramble(&mut data, &ripemd::digest(&seed));
        }
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(data.as_slice())
            .take(UNPACKED_LIMIT)
            .read_to_end(&mut out)
            .map_err(|e| bad(path, e.to_string()))?;
        out
    } else {
        info.to_vec()
    };
    let sizes = key_block_sizes(path, &info, blocks, wide, header.utf16)?;
    let mut area = Cursor::new(cursor.take(keys_size).ok_or_else(cut)?);
    let mut keys = Vec::new();
    for (compressed, unpacked) in sizes {
        let block = area.take(compressed).ok_or_else(cut)?;
        let block = unpack(path, block, unpacked)?;
        keys_in(path, &block, wide, header.utf16, &mut keys)?;
    }
    Ok((keys, cursor))
}

/// The definitions, one after the other.
fn read_records(path: &Path, wide: bool, mut cursor: Cursor<'_>) -> Result<Vec<u8>, ImportError> {
    let cut = || bad(path, "the definitions are cut short");
    let blocks = cursor.number(wide).ok_or_else(cut)?;
    let _entries = cursor.number(wide).ok_or_else(cut)?;
    let info_size = cursor.number(wide).ok_or_else(cut)?;
    let _size = cursor.number(wide).ok_or_else(cut)?;
    let mut info = Cursor::new(cursor.take(info_size).ok_or_else(cut)?);
    let mut out = Vec::new();
    for _ in 0..blocks {
        let compressed = info.number(wide).ok_or_else(cut)?;
        let unpacked = info.number(wide).ok_or_else(cut)?;
        let block = cursor.take(compressed).ok_or_else(cut)?;
        out.extend_from_slice(&unpack(path, block, unpacked)?);
        if out.len() as u64 > 4 * UNPACKED_LIMIT {
            return Err(bad(path, "too large once unpacked"));
        }
    }
    Ok(out)
}

pub(super) fn read_terms(path: &Path, cancel: &Cancel) -> Result<Vec<Term>, ImportError> {
    let bytes = read_file(path)?;
    let (header, cursor) = open(path, &bytes)?;
    let wide = header.version >= 2.0;
    let (keys, cursor) = read_keys(path, &header, cursor)?;
    let records = read_records(path, wide, cursor)?;
    let record = |i: usize| -> Option<String> {
        let start = usize::try_from(keys[i].0).ok()?;
        let end = keys
            .get(i + 1)
            .map_or(Some(records.len()), |k| usize::try_from(k.0).ok())?;
        let text = text_of(records.get(start..end)?, header.utf16);
        Some(text.trim_end_matches(['\0', '\r', '\n']).to_string())
    };
    let places: HashMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.1.as_str(), i))
        .collect();
    let mut terms = Terms::new(cancel);
    for (i, key) in keys.iter().enumerate() {
        let Some(mut text) = record(i) else {
            return Err(bad(path, "an entry is outside the definitions"));
        };
        // `@@@LINK=word`: the entry of that word, if it is found
        for _ in 0..LINK_DEPTH {
            let Some(word) = text.strip_prefix("@@@LINK=") else {
                break;
            };
            text = places
                .get(word.trim())
                .and_then(|at| record(*at))
                .unwrap_or_default();
        }
        if text.starts_with("@@@LINK=") {
            continue;
        }
        terms.add(&key.1, &html_to_text(&text))?;
    }
    Ok(terms.finish())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn deflate(data: &[u8]) -> Vec<u8> {
        let mut out = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        out.write_all(data).unwrap();
        out.finish().unwrap()
    }

    fn block(data: &[u8], packed: bool) -> Vec<u8> {
        let mut out = if packed {
            vec![2, 0, 0, 0]
        } else {
            vec![0, 0, 0, 0]
        };
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out.extend_from_slice(&if packed { deflate(data) } else { data.to_vec() });
        out
    }

    /// The reverse of `unscramble`.
    fn scramble(data: &mut [u8], key: &[u8; 16]) {
        let mut previous = 0x36u8;
        for (i, byte) in data.iter_mut().enumerate() {
            let plain = *byte ^ previous ^ (i as u8) ^ key[i % key.len()];
            *byte = plain.rotate_left(4);
            previous = *byte;
        }
    }

    /// A dictionary in the shape of writemdict's, of engine 2.0 or 1.2.
    fn file(entries: &[(&str, &str)], wide: bool, encrypted: u32, utf16: bool) -> Vec<u8> {
        let number = |n: u64| {
            if wide {
                n.to_be_bytes().to_vec()
            } else {
                (n as u32).to_be_bytes().to_vec()
            }
        };
        let encode = |text: &str| -> Vec<u8> {
            if utf16 {
                text.encode_utf16().flat_map(u16::to_le_bytes).collect()
            } else {
                text.as_bytes().to_vec()
            }
        };
        let end: &[u8] = if utf16 { &[0, 0] } else { &[0] };
        let (mut keys, mut records) = (Vec::new(), Vec::new());
        for (word, text) in entries {
            keys.extend_from_slice(&number(records.len() as u64));
            keys.extend_from_slice(&encode(word));
            keys.extend_from_slice(end);
            records.extend_from_slice(&encode(text));
            records.extend_from_slice(end);
        }
        let key_block = block(&keys, true);
        // one block of headwords, first and last words, sizes
        let mut info = number(entries.len() as u64);
        for word in [entries[0].0, entries[entries.len() - 1].0] {
            let chars = if utf16 {
                word.encode_utf16().count()
            } else {
                word.len()
            };
            if wide {
                info.extend_from_slice(&(chars as u16).to_be_bytes());
            } else {
                info.push(chars as u8);
            }
            info.extend_from_slice(&encode(word));
            if wide {
                info.extend_from_slice(end);
            }
        }
        info.extend_from_slice(&number(key_block.len() as u64));
        info.extend_from_slice(&number(keys.len() as u64));
        let info_bytes = if wide {
            let sum = adler32(&info).to_be_bytes();
            let mut data = deflate(&info);
            if encrypted & 2 != 0 {
                let mut seed = sum.to_vec();
                seed.extend_from_slice(&0x3695u32.to_le_bytes());
                scramble(&mut data, &ripemd::digest(&seed));
            }
            let mut out = vec![2, 0, 0, 0];
            out.extend_from_slice(&sum);
            out.extend_from_slice(&data);
            out
        } else {
            info.clone()
        };
        let xml = format!(
            "<Dictionary GeneratedByEngineVersion=\"{}\" Encrypted=\"{encrypted}\" Encoding=\"{}\" Title=\"Mini MDict\" Description=\"&lt;b&gt;x\"/>",
            if wide { "2.0" } else { "1.2" },
            if utf16 { "UTF-16" } else { "UTF-8" },
        );
        let header: Vec<u8> = xml.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut out = (header.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(&header);
        out.extend_from_slice(&adler32(&header).to_le_bytes());
        let mut section = number(1);
        section.extend_from_slice(&number(entries.len() as u64));
        if wide {
            section.extend_from_slice(&(info.len() as u64).to_be_bytes());
        }
        section.extend_from_slice(&number(info_bytes.len() as u64));
        section.extend_from_slice(&number(key_block.len() as u64));
        out.extend_from_slice(&section);
        if wide {
            out.extend_from_slice(&adler32(&section).to_be_bytes());
        }
        out.extend_from_slice(&info_bytes);
        out.extend_from_slice(&key_block);
        let record_block = block(&records, true);
        let mut info = number(record_block.len() as u64);
        info.extend_from_slice(&number(records.len() as u64));
        out.extend_from_slice(&number(1));
        out.extend_from_slice(&number(entries.len() as u64));
        out.extend_from_slice(&number(info.len() as u64));
        out.extend_from_slice(&number(record_block.len() as u64));
        out.extend_from_slice(&info);
        out.extend_from_slice(&record_block);
        out
    }

    const ENTRIES: [(&str, &str); 3] = [
        ("たべる【食べる】", "<b>to eat</b><br>food"),
        ("喰べる", "@@@LINK=たべる【食べる】"),
        ("飲む", "to drink"),
    ];

    fn read(bytes: &[u8]) -> Result<Vec<Term>, ImportError> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.mdx");
        std::fs::write(&path, bytes).unwrap();
        read_index(&path)?;
        read_terms(&path, &Cancel::new())
    }

    // T-DIC-004: engines 2.0 and 1.2, an obscured index, UTF-8 and UTF-16
    #[test]
    fn an_mdict_is_read() {
        for (wide, encrypted, utf16) in [
            (true, 0, false),
            (true, 2, false),
            (true, 2, true),
            (false, 0, false),
            (false, 0, true),
        ] {
            let terms = read(&file(&ENTRIES, wide, encrypted, utf16))
                .unwrap_or_else(|e| panic!("{wide} {encrypted} {utf16}: {e}"));
            let words: Vec<_> = terms
                .iter()
                .map(|t| {
                    (
                        t.expression.as_str(),
                        t.reading.as_str(),
                        t.glossary.as_str(),
                    )
                })
                .collect();
            assert_eq!(
                words,
                [
                    ("食べる", "たべる", r#"["to eat\nfood"]"#),
                    ("喰べる", "", r#"["to eat\nfood"]"#),
                    ("飲む", "", r#"["to drink"]"#),
                ],
                "{wide} {encrypted} {utf16}"
            );
        }
    }

    #[test]
    fn its_title_comes_from_its_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.mdx");
        std::fs::write(&path, file(&ENTRIES, true, 2, false)).unwrap();
        assert_eq!(read_index(&path).unwrap().title, "Mini MDict");
    }

    // T-DIC-004
    #[test]
    fn what_is_not_read_says_so() {
        let locked = file(&ENTRIES, true, 1, false);
        assert!(read(&locked).unwrap_err().to_string().contains("locked"));
        assert!(read(b"not at all").is_err());
        // damage anywhere after the header is an error
        let good = file(&ENTRIES, true, 2, false);
        for at in [good.len() - 1, good.len() / 2, good.len() - 40] {
            let mut bad = good.clone();
            bad[at] ^= 0xff;
            assert!(read(&bad).is_err(), "{at}");
        }
        assert!(read(&good[..good.len() / 2]).is_err());
    }

    #[test]
    fn a_link_to_nothing_or_a_loop_adds_nothing() {
        let entries = [
            ("あ", "@@@LINK=い"),
            ("い", "@@@LINK=あ"),
            ("う", "@@@LINK=ない"),
        ];
        assert!(read(&file(&entries, true, 0, false)).unwrap().is_empty());
    }

    // T-DIC-004
    #[test]
    fn the_checksum_is_adler32() {
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }
}
