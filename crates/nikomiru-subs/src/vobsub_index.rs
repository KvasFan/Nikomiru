// SPDX-License-Identifier: GPL-3.0-only

//! External VobSub index metadata and track selection (SUB-031). Paths in
//! index text are never followed; only offsets into its local companion count.

use crate::vobsub::{Palette, palette};

/// Largest UTF-8 index accepted.
pub const MAX_BYTES: usize = 1 << 20;
/// Largest number of timestamp records.
pub const MAX_CUES: usize = 100_000;

/// One index entry; its timestamp supersedes the MPEG packet PTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Milliseconds from the video start.
    pub time_ms: i64,
    /// Offset into the companion program stream.
    pub file_pos: usize,
}

/// One language/private stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// ISO language tag from the index.
    pub language: String,
    /// VobSub substream index, 0…31 (packet id is 0x20 plus this).
    pub index: u8,
    /// Time/offset entries in file order.
    pub entries: Vec<Entry>,
}

/// Parsed local index, without any filesystem access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// The shared RGB palette; missing means the DVD fallback palette.
    pub palette: Option<Palette>,
    /// Language tracks in index order.
    pub tracks: Vec<Track>,
    /// `langidx`, if present and valid.
    pub default: Option<u8>,
}

fn time(value: &str) -> Option<i64> {
    let fields: Vec<_> = value.split(':').collect();
    if fields.len() != 4 {
        return None;
    }
    let h = fields[0].parse::<i64>().ok()?;
    let m = fields[1].parse::<i64>().ok()?;
    let s = fields[2].parse::<i64>().ok()?;
    let ms = fields[3].parse::<i64>().ok()?;
    ((0..=9999).contains(&h)
        && (0..60).contains(&m)
        && (0..60).contains(&s)
        && (0..1000).contains(&ms))
    .then_some(h * 3_600_000 + m * 60_000 + s * 1000 + ms)
}

impl Index {
    /// Parse an index as a whole; malformed records never produce a partial
    /// track. Duplicate stream ids and decreasing timestamps/offsets fail.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > MAX_BYTES
            || !text
                .trim_start_matches('\u{feff}')
                .starts_with("# VobSub index file")
        {
            return None;
        }
        let mut out = Self {
            palette: None,
            tracks: Vec::new(),
            default: None,
        };
        let mut count = 0;
        for line in text.lines().map(str::trim) {
            if let Some(value) = line.strip_prefix("langidx:") {
                let index = value.trim().parse::<u8>().ok()?;
                if index >= 32 {
                    return None;
                }
                out.default = Some(index);
            } else if line.starts_with("palette:") {
                out.palette = Some(palette(line)?);
            } else if let Some(value) = line.strip_prefix("id:") {
                let (language, index) = value.split_once(", index:")?;
                let index = index.trim().parse::<u8>().ok()?;
                if index >= 32 || out.tracks.iter().any(|t| t.index == index) {
                    return None;
                }
                out.tracks.push(Track {
                    language: language.trim().into(),
                    index,
                    entries: Vec::new(),
                });
            } else if let Some(value) = line.strip_prefix("timestamp:") {
                let (timestamp, offset) = value.trim().split_once(", filepos:")?;
                let entry = Entry {
                    time_ms: time(timestamp.trim())?,
                    file_pos: usize::from_str_radix(offset.trim(), 16).ok()?,
                };
                let entries = &mut out.tracks.last_mut()?.entries;
                if entries.last().is_some_and(|old| {
                    old.time_ms > entry.time_ms || old.file_pos >= entry.file_pos
                }) {
                    return None;
                }
                count += 1;
                if count > MAX_CUES {
                    return None;
                }
                entries.push(entry);
            }
        }
        (!out.tracks.is_empty()).then_some(out)
    }

    /// Preferred language, explicit index default, then first nonempty track.
    pub fn choose(&self, preferred: &[String]) -> Option<&Track> {
        let available = || self.tracks.iter().filter(|track| !track.entries.is_empty());
        preferred
            .iter()
            .find_map(|lang| {
                available().find(|track| crate::select::same_language(lang, &track.language))
            })
            .or_else(|| available().find(|track| Some(track.index) == self.default))
            .or_else(|| available().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pgs::{Graphic, testing::block};
    use crate::vobsub::{self, testing as dvd};

    // T-SUB-031: sparse private ids and index times supersede MPEG PTS;
    // another language's packets are never substituted.
    #[test]
    fn multilingual_indices_choose_and_decode_only_the_selected_track() {
        let (mut bytes, first) =
            dvd::program_stream_track(&[(1000, dvd::spu(0, 0, &block(80, 20), 1000))], 2000, 0);
        let offset = bytes.len();
        let (second, starts) =
            dvd::program_stream_track(&[(2000, dvd::spu(0, 0, &block(120, 20), 1000))], 2000, 3);
        bytes.extend(second);
        let mut text = dvd::idx(&dvd::PALETTE, &[5000], &first)
            .replace("id: ja, index: 0", "langidx: 3\nid: en, index: 0");
        text.push_str(&format!(
            "id: ja, index: 3\ntimestamp: 00:00:10:000, filepos: {:09x}\n",
            offset + starts[0]
        ));
        let index = Index::parse(&text).unwrap();
        let jp = index.choose(&["jpn".into()]).unwrap();
        assert_eq!(jp.index, 3);
        let cues = vobsub::parse_indexed(&bytes, &index, jp).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 10_000);
        assert!(cues[0].picture().unwrap().width >= 120);
        let en = index.choose(&["en".into()]).unwrap();
        assert_eq!(
            vobsub::parse_indexed(&bytes, &index, en).unwrap()[0].start_ms,
            5000
        );
        assert_eq!(index.choose(&["fr".into()]).unwrap().index, 3);
        let mut bad = jp.clone();
        bad.entries[0].file_pos = bytes.len() + 1;
        assert!(vobsub::parse_indexed(&bytes, &index, &bad).is_none());
        bad = jp.clone();
        bad.index = 2;
        assert!(vobsub::parse_indexed(&bytes, &index, &bad).is_none());
    }

    // T-SUB-031: malformed records, duplicates and source bounds reject the
    // whole index, without a usable truncated prefix.
    #[test]
    fn malformed_or_excessive_indices_are_not_partially_accepted() {
        for text in [
            "not an index",
            "# VobSub index file\nlangidx: 32\nid: ja, index: 0",
            "# VobSub index file\nid: ja, index: 32",
            "# VobSub index file\nid: ja, index: 0\nid: en, index: 0",
            "# VobSub index file\ntimestamp: 00:00:01:000, filepos: 00",
            "# VobSub index file\nid: ja, index: 0\ntimestamp: 00:99:00:000, filepos: 00",
            "# VobSub index file\nid: ja, index: 0\ntimestamp: 00:00:02:000, filepos: 10\ntimestamp: 00:00:01:000, filepos: 20",
            "# VobSub index file\nid: ja, index: 0\ntimestamp: 00:00:01:000, filepos: 20\ntimestamp: 00:00:02:000, filepos: 10",
        ] {
            assert!(Index::parse(text).is_none(), "{text}");
        }
        let too_large = format!("# VobSub index file\n{}", "x".repeat(MAX_BYTES));
        assert!(Index::parse(&too_large).is_none());
    }
}
