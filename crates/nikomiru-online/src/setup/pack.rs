// SPDX-License-Identifier: GPL-3.0-only

//! Fushi's recommended pack, read by range (INS-003, INS-004): its manifest,
//! as Fushi's `fushi/lib/src/onboarding/recommended_pack.dart` reads it (the
//! whole pack's address and mirrors, its physical parts and their
//! addresses), the requests a range of the pack becomes, and the ZIP central
//! directory read from the pack's tail. Nothing here touches the network.

use reqwest::Url;

use crate::error::OnlineError;

/// The largest manifest read.
pub const MAX_MANIFEST: usize = 64 << 10;
/// The largest pack taken.
pub const MAX_PACK: u64 = 20 << 30;
/// The largest central directory read.
pub const MAX_DIRECTORY: u64 = 16 << 20;
/// The largest entry downloaded.
pub const MAX_ENTRY: u64 = 12 << 30;
/// The largest number of parts.
const MAX_PARTS: usize = 4096;

/// A physical part of the pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Its file name, joined to each part address.
    pub name: String,
    /// Where it starts in the pack.
    pub offset: u64,
    /// Its length.
    pub length: u64,
}

/// The pack a manifest points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Its version, as the manifest names it.
    pub version: String,
    /// The whole pack's addresses: the main one, then the mirrors.
    pub whole: Vec<Url>,
    /// Its size.
    pub size: u64,
    /// Its parts, contiguous from 0 to `size`; empty when it has none.
    pub parts: Vec<Part>,
    /// The folders the parts are in, mirrors of each other.
    pub part_bases: Vec<Url>,
}

fn bad(reason: impl Into<String>) -> OnlineError {
    OnlineError::Protocol {
        provider: "pack".into(),
        reason: reason.into(),
    }
}

fn address(text: &str) -> Option<Url> {
    crate::http::checked_url(text).ok()
}

fn addresses(value: &serde_json::Value) -> Vec<Url> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(|v| address(v.as_str()?)).collect())
        .unwrap_or_default()
}

/// The parts of a manifest, or none when one is not right: a name with a
/// `/`, a gap, an overlap, a total that is not the pack's.
fn parts(value: &serde_json::Value, size: u64) -> Vec<Part> {
    let Some(list) = value.as_array() else {
        return Vec::new();
    };
    if list.len() > MAX_PARTS {
        return Vec::new();
    }
    let mut parts = Vec::with_capacity(list.len());
    let mut next = 0u64;
    for item in list {
        let name = item["name"].as_str().unwrap_or_default();
        let (Some(offset), Some(length)) = (item["offset"].as_u64(), item["length"].as_u64())
        else {
            return Vec::new();
        };
        if name.is_empty() || name.contains(['/', '\\']) || name == ".." || length == 0 {
            return Vec::new();
        }
        if offset != next {
            return Vec::new();
        }
        next = offset.saturating_add(length);
        parts.push(Part {
            name: name.to_string(),
            offset,
            length,
        });
    }
    if next != size {
        return Vec::new();
    }
    parts
}

/// The pack of a manifest's JSON; `None` when it is not one Nikomiru can
/// read: no HTTPS address, no size, or a size beyond [`MAX_PACK`]. The
/// `deflate_pack` that Fushi also describes is left aside: its entries are
/// compressed, and only stored ones are taken (INS-004).
pub fn parse_manifest(text: &str) -> Option<Manifest> {
    if text.len() > MAX_MANIFEST {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let main = address(value["url"].as_str()?)?;
    let size = value["size_bytes"]
        .as_u64()
        .filter(|&s| s > 0 && s <= MAX_PACK)?;
    let mut whole = vec![main];
    whole.extend(addresses(&value["mirrors"]));
    let part_bases = addresses(&value["part_base_urls"]);
    let parts = if part_bases.is_empty() {
        Vec::new()
    } else {
        parts(&value["parts"], size)
    };
    let part_bases = if parts.is_empty() {
        Vec::new()
    } else {
        part_bases
    };
    Some(Manifest {
        version: value["version"].as_str().unwrap_or_default().to_string(),
        whole,
        size,
        parts,
        part_bases,
    })
}

/// A request for a piece of the pack: where it is in the pack, its length,
/// and where to ask for it, in order (an address and the offset there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Its place in the pack.
    pub offset: u64,
    /// Its length.
    pub len: u64,
    /// The addresses that have it, with its offset in each.
    pub sources: Vec<(Url, u64)>,
}

fn joined(base: &Url, name: &str) -> Option<Url> {
    let mut text = base.as_str().to_string();
    if !text.ends_with('/') {
        text.push('/');
    }
    Url::parse(&format!("{text}{name}")).ok()
}

impl Manifest {
    /// The requests that fetch `len` bytes of the pack from `from`: never
    /// across a part, never longer than `chunk`; each from the part's
    /// addresses first, then from the whole pack's.
    pub fn spans(&self, from: u64, len: u64, chunk: u64) -> Vec<Span> {
        let end = from.saturating_add(len).min(self.size);
        let chunk = chunk.max(1);
        let mut spans = Vec::new();
        let mut at = from;
        while at < end {
            let part = self
                .parts
                .iter()
                .find(|p| p.offset <= at && at < p.offset + p.length);
            let limit = part.map_or(end, |p| (p.offset + p.length).min(end));
            let len = (limit - at).min(chunk);
            let mut sources: Vec<(Url, u64)> = Vec::new();
            if let Some(p) = part {
                for base in &self.part_bases {
                    if let Some(url) = joined(base, &p.name) {
                        sources.push((url, at - p.offset));
                    }
                }
            }
            sources.extend(self.whole.iter().map(|u| (u.clone(), at)));
            spans.push(Span {
                offset: at,
                len,
                sources,
            });
            at += len;
        }
        spans
    }
}

/// An entry of the pack's ZIP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name in the ZIP: only compared, never a path.
    pub name: String,
    /// 0 for stored.
    pub method: u16,
    /// Its CRC-32.
    pub crc32: u32,
    /// Its length in the ZIP.
    pub compressed: u64,
    /// Its length.
    pub size: u64,
    /// Where its local header starts.
    pub header: u64,
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// How much of the tail of a pack of `size` bytes to read for its end of
/// central directory: the record, its longest comment, ZIP64's locator.
pub fn tail_length(size: u64) -> u64 {
    size.min(22 + 0xffff + 20)
}

/// Where the central directory is: its offset, its length, its entries,
/// from `tail`, the last bytes of a pack of `size` bytes.
pub fn locate_directory(tail: &[u8], size: u64) -> Result<(u64, u64, u64), OnlineError> {
    let base = size
        .checked_sub(tail.len() as u64)
        .ok_or_else(|| bad("tail longer than the pack"))?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| tail[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(|| bad("no end of central directory"))?;
    let field = |i| u32_at(tail, eocd + i).ok_or_else(|| bad("short end of directory"));
    let count = u16_at(tail, eocd + 10).ok_or_else(|| bad("short end of directory"))?;
    let (mut offset, mut length, mut entries) = (
        u64::from(field(16)?),
        u64::from(field(12)?),
        u64::from(count),
    );
    // ZIP64: the locator just before, then the record it points to
    if eocd >= 20 && tail[eocd - 20..].starts_with(b"PK\x06\x07") {
        let record = u64_at(tail, eocd - 20 + 8).ok_or_else(|| bad("short ZIP64 locator"))?;
        let at = record
            .checked_sub(base)
            .and_then(|i| usize::try_from(i).ok())
            .filter(|&i| {
                tail.get(i..i + 56)
                    .is_some_and(|r| r.starts_with(b"PK\x06\x06"))
            })
            .ok_or_else(|| bad("ZIP64 record not in the tail"))?;
        entries = u64_at(tail, at + 32).ok_or_else(|| bad("short ZIP64 record"))?;
        length = u64_at(tail, at + 40).ok_or_else(|| bad("short ZIP64 record"))?;
        offset = u64_at(tail, at + 48).ok_or_else(|| bad("short ZIP64 record"))?;
    }
    if length > MAX_DIRECTORY || offset.saturating_add(length) > size {
        return Err(bad("central directory out of bounds"));
    }
    Ok((offset, length, entries))
}

/// The entries of a central directory.
pub fn parse_directory(bytes: &[u8], entries: u64) -> Result<Vec<Entry>, OnlineError> {
    let short = || bad("short central directory");
    let mut out = Vec::new();
    let mut i = 0usize;
    for _ in 0..entries {
        if !bytes.get(i..).is_some_and(|b| b.starts_with(b"PK\x01\x02")) {
            return Err(bad("not a central directory entry"));
        }
        let method = u16_at(bytes, i + 10).ok_or_else(short)?;
        let crc32 = u32_at(bytes, i + 16).ok_or_else(short)?;
        let mut compressed = u64::from(u32_at(bytes, i + 20).ok_or_else(short)?);
        let mut size = u64::from(u32_at(bytes, i + 24).ok_or_else(short)?);
        let name_len = usize::from(u16_at(bytes, i + 28).ok_or_else(short)?);
        let extra_len = usize::from(u16_at(bytes, i + 30).ok_or_else(short)?);
        let comment_len = usize::from(u16_at(bytes, i + 32).ok_or_else(short)?);
        let mut header = u64::from(u32_at(bytes, i + 42).ok_or_else(short)?);
        let name = bytes.get(i + 46..i + 46 + name_len).ok_or_else(short)?;
        let extra = bytes
            .get(i + 46 + name_len..i + 46 + name_len + extra_len)
            .ok_or_else(short)?;
        // ZIP64's extra field holds the values written as 0xffffffff
        let mut j = 0;
        while j + 4 <= extra.len() {
            let tag = u16_at(extra, j).ok_or_else(short)?;
            let len = usize::from(u16_at(extra, j + 2).ok_or_else(short)?);
            if tag == 1 {
                let mut k = j + 4;
                for value in [&mut size, &mut compressed, &mut header] {
                    if *value == u64::from(u32::MAX) {
                        *value = u64_at(extra, k).ok_or_else(short)?;
                        k += 8;
                    }
                }
            }
            j += 4 + len;
        }
        out.push(Entry {
            name: String::from_utf8_lossy(name).into_owned(),
            method,
            crc32,
            compressed,
            size,
            header,
        });
        i += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// Where an entry's bytes start, from the first 30 bytes of its local
/// header at `header`.
pub fn data_offset(local: &[u8], header: u64) -> Result<u64, OnlineError> {
    if !local.starts_with(b"PK\x03\x04") {
        return Err(bad("not a local header"));
    }
    let name = u16_at(local, 26).ok_or_else(|| bad("short local header"))?;
    let extra = u16_at(local, 28).ok_or_else(|| bad("short local header"))?;
    Ok(header + 30 + u64::from(name) + u64::from(extra))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(parts: &str) -> String {
        format!(
            r#"{{"version": "v1", "url": "https://example.org/pack.zip", "size_bytes": 300,
                "mirrors": ["https://mirror.example.org/pack.zip", "http://example.org/x"],
                "part_base_urls": ["https://parts.example.org/v1/"], "parts": {parts}}}"#
        )
    }

    // T-INS-003: a manifest read, its parts kept only when they hold together
    #[test]
    fn a_manifest_and_its_parts() {
        let good = r#"[{"name": "p.000", "offset": 0, "length": 200},
                       {"name": "p.001", "offset": 200, "length": 100}]"#;
        let m = parse_manifest(&manifest(good)).unwrap();
        assert_eq!(m.version, "v1");
        assert_eq!(m.whole.len(), 2, "plain HTTP off this computer is refused");
        assert_eq!(m.parts.len(), 2);
        for bad_parts in [
            r#"[{"name": "p.000", "offset": 0, "length": 200}]"#,
            r#"[{"name": "../x", "offset": 0, "length": 300}]"#,
            r#"[{"name": "a", "offset": 0, "length": 100}, {"name": "b", "offset": 150, "length": 150}]"#,
            r#"[{"name": "a", "offset": 0}]"#,
        ] {
            let m = parse_manifest(&manifest(bad_parts)).unwrap();
            assert!(m.parts.is_empty() && m.part_bases.is_empty(), "{bad_parts}");
        }
        assert!(parse_manifest(r#"{"url": "http://example.org/p", "size_bytes": 3}"#).is_none());
        assert!(parse_manifest(r#"{"url": "https://example.org/p"}"#).is_none());
        let huge = format!(
            r#"{{"url": "https://e.org/p", "size_bytes": {}}}"#,
            MAX_PACK + 1
        );
        assert!(parse_manifest(&huge).is_none());
        assert!(parse_manifest("{").is_none());
    }

    // T-INS-003: a range of the pack asked of its parts, then of the whole
    #[test]
    fn a_range_becomes_requests_on_the_parts() {
        let parts = r#"[{"name": "p.000", "offset": 0, "length": 200},
                        {"name": "p.001", "offset": 200, "length": 100}]"#;
        let m = parse_manifest(&manifest(parts)).unwrap();
        let spans = m.spans(150, 120, 64);
        let got: Vec<(u64, u64)> = spans.iter().map(|s| (s.offset, s.len)).collect();
        assert_eq!(got, [(150, 50), (200, 64), (264, 6)]);
        assert_eq!(
            spans[1].sources[0],
            (Url::parse("https://parts.example.org/v1/p.001").unwrap(), 0)
        );
        assert_eq!(spans[2].sources[0].1, 64);
        assert_eq!(
            spans[2].sources[1],
            (Url::parse("https://example.org/pack.zip").unwrap(), 264)
        );
        assert_eq!(
            m.spans(290, 50, 64).last().unwrap().len,
            10,
            "not past the end"
        );
    }

    // T-INS-003: a ZIP64 central directory read from the tail alone
    #[test]
    fn the_directory_from_the_tail() {
        let pack =
            crate::fake_files::zip64(&[("fushi.db", b"db"), ("localAudio/a.db", b"audio bytes")]);
        let size = pack.len() as u64;
        let tail = &pack[pack.len() - tail_length(size) as usize..];
        let (offset, length, count) = locate_directory(tail, size).unwrap();
        assert_eq!(count, 2);
        let directory = &pack[offset as usize..(offset + length) as usize];
        let entries = parse_directory(directory, count).unwrap();
        assert_eq!(entries[1].name, "localAudio/a.db");
        assert_eq!((entries[1].size, entries[1].method), (11, 0));
        assert_eq!(entries[1].crc32, crc32fast::hash(b"audio bytes"));
        let header = entries[1].header as usize;
        let at = data_offset(&pack[header..header + 30], entries[1].header).unwrap() as usize;
        assert_eq!(&pack[at..at + 11], b"audio bytes");
        assert!(locate_directory(b"no zip here, none at all", 24).is_err());
        assert!(parse_directory(b"PK\x01\x02short", 1).is_err());
        assert!(data_offset(b"nope", 0).is_err());
    }
}
