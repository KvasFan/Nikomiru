// SPDX-License-Identifier: GPL-3.0-only

//! The cards kept for later (ANK-044), as Fushi's "mine after watching"
//! (`fushi/lib/src/mining/video_mine_queue.dart`): each one a folder of
//! `$XDG_DATA_HOME/nikomiru/later` named by a number that grows with each
//! card, holding `card.json` and the card's media under the names of ANK-036.
//! A folder is written under a hidden name and renamed once complete, so a
//! card is never half kept.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use nikomiru_ui::mining::{LaterCard, MineRequest};
use serde::{Deserialize, Serialize};

/// A media file of a card: its name and its bytes.
pub type Media = (String, Vec<u8>);

/// A card made and not sent yet: what the `anki` worker sends to Anki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The value of each placeholder but the media's (`compose::values`).
    pub values: BTreeMap<String, String>,
    /// The dictionaries' images.
    pub media: Vec<Media>,
    /// The sentence audio.
    pub audio: Option<Media>,
    /// The picture.
    pub image: Option<Media>,
    /// What `mined_cards` records once it is sent (ANK-039).
    pub record: Record,
}

/// What `mined_cards` records of a card (ANK-039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The headword.
    pub expression: String,
    /// Its reading.
    pub reading: String,
    /// The reference cue's text.
    pub sentence: String,
    /// The video's uid in the library.
    pub uid: Option<String>,
    /// The clip, in player milliseconds.
    pub start_ms: i64,
    /// Its end.
    pub end_ms: i64,
    /// The `sourceId` of the card's link (ANK-050).
    pub source_id: String,
}

impl Record {
    /// The record of the card of `r`.
    pub fn of(r: &MineRequest, source_id: &str, (start_ms, end_ms): (i64, i64)) -> Self {
        Self {
            expression: r.expression.clone(),
            reading: r.reading.clone(),
            sentence: r.sentence.clone(),
            uid: r.uid.clone(),
            start_ms,
            end_ms,
            source_id: source_id.to_string(),
        }
    }
}

/// The version of `card.json`.
const FORMAT: u32 = 1;

const CARD: &str = "card.json";

/// `card.json`.
#[derive(Debug, Serialize, Deserialize)]
struct CardFile {
    format: u32,
    /// When it was kept, in milliseconds since the epoch.
    created_ms: i64,
    values: BTreeMap<String, String>,
    /// The dictionaries' images, files of the folder.
    media: Vec<String>,
    audio: Option<String>,
    image: Option<String>,
    record: Record,
    /// Made with Add anyway.
    #[serde(default)]
    anyway: bool,
    /// Why its last send failed.
    #[serde(default)]
    problem: Option<String>,
}

/// The name of a card's folder: digits only.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

/// A media name as ANK-036 makes them, `nikomiru_<16 hex digits>.<ext>`:
/// nothing else is read from or written to a folder.
fn valid_media(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("nikomiru_") else {
        return false;
    };
    let Some((hex, ext)) = rest.split_once('.') else {
        return false;
    };
    hex.len() == 16
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
        && (1..=5).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn invalid(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

/// The folders of the cards, by their number.
fn ids(dir: &Path) -> Vec<(u64, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<(u64, String)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            valid_id(&name).then(|| Some((name.parse().ok()?, name)))?
        })
        .collect();
    ids.sort();
    ids
}

/// Keeps a card: its folder's name, the next number.
pub fn keep(dir: &Path, p: &Prepared, anyway: bool, now: SystemTime) -> io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let next = ids(dir).last().map_or(1, |(n, _)| n + 1);
    let id = format!("{next:010}");
    let part = dir.join(format!(".{id}.part"));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir(&part)?;
    let write = || -> io::Result<()> {
        for (name, bytes) in p.media.iter().chain(&p.audio).chain(&p.image) {
            if !valid_media(name) {
                return Err(invalid(format!("media name {name:?}")));
            }
            std::fs::write(part.join(name), bytes)?;
        }
        let file = CardFile {
            format: FORMAT,
            created_ms: now
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX)),
            values: p.values.clone(),
            media: p.media.iter().map(|(n, _)| n.clone()).collect(),
            audio: p.audio.as_ref().map(|(n, _)| n.clone()),
            image: p.image.as_ref().map(|(n, _)| n.clone()),
            record: p.record.clone(),
            anyway,
            problem: None,
        };
        std::fs::write(part.join(CARD), serde_json::to_vec_pretty(&file)?)?;
        std::fs::rename(&part, dir.join(&id))
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&part);
    })?;
    Ok(id)
}

/// `card.json` of a folder, its media checked: named as ANK-036 and there.
fn read(dir: &Path, id: &str) -> io::Result<CardFile> {
    if !valid_id(id) {
        return Err(invalid(format!("card {id:?}")));
    }
    let folder = dir.join(id);
    let file: CardFile = serde_json::from_slice(&std::fs::read(folder.join(CARD))?)?;
    if file.format > FORMAT {
        return Err(invalid(format!("format {}", file.format)));
    }
    for name in file.media.iter().chain(&file.audio).chain(&file.image) {
        if !valid_media(name) {
            return Err(invalid(format!("media name {name:?}")));
        }
        if !folder.join(name).is_file() {
            return Err(invalid(format!("{name} is missing")));
        }
    }
    Ok(file)
}

/// The cards kept, in the order they were made; a folder that cannot be
/// read is listed as damaged.
pub fn list(dir: &Path) -> Vec<LaterCard> {
    ids(dir)
        .into_iter()
        .map(|(_, id)| match read(dir, &id) {
            Ok(file) => LaterCard {
                expression: file.record.expression,
                reading: file.record.reading,
                sentence: file
                    .record
                    .sentence
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                problem: file.problem,
                damaged: false,
                id,
            },
            Err(e) => {
                tracing::warn!("card for later {id}: {e}");
                LaterCard {
                    id,
                    damaged: true,
                    ..LaterCard::default()
                }
            }
        })
        .collect()
}

/// A card kept, to send, and whether it was made with Add anyway.
pub fn load(dir: &Path, id: &str) -> Result<(Prepared, bool), String> {
    let file = read(dir, id).map_err(|e| e.to_string())?;
    let folder = dir.join(id);
    let media = |name: &String| -> Result<Media, String> {
        std::fs::read(folder.join(name))
            .map(|bytes| (name.clone(), bytes))
            .map_err(|e| format!("{name}: {e}"))
    };
    let prepared = Prepared {
        values: file.values,
        media: file.media.iter().map(media).collect::<Result<_, _>>()?,
        audio: file.audio.as_ref().map(media).transpose()?,
        image: file.image.as_ref().map(media).transpose()?,
        record: file.record,
    };
    Ok((prepared, file.anyway))
}

/// Removes a card kept.
pub fn forget(dir: &Path, id: &str) -> io::Result<()> {
    if !valid_id(id) {
        return Err(invalid(format!("card {id:?}")));
    }
    std::fs::remove_dir_all(dir.join(id))
}

/// Notes why a card was not sent.
pub fn note_problem(dir: &Path, id: &str, problem: &str) -> io::Result<()> {
    let mut file = read(dir, id)?;
    file.problem = Some(problem.to_string());
    let folder = dir.join(id);
    let part = folder.join(".card.json.part");
    std::fs::write(&part, serde_json::to_vec_pretty(&file)?)?;
    std::fs::rename(&part, folder.join(CARD))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(word: &str, audio: &[u8]) -> Prepared {
        Prepared {
            values: BTreeMap::from([
                ("expression".to_string(), word.to_string()),
                ("glossary".to_string(), "<ol><li>meal</li></ol>".to_string()),
            ]),
            media: vec![("nikomiru_00112233445566ff.png".into(), b"png".to_vec())],
            audio: Some((nikomiru_anki::media::name(audio, "m4a"), audio.to_vec())),
            image: None,
            record: Record {
                expression: word.into(),
                reading: "ごはん".into(),
                sentence: "ご飯を\n食べた".into(),
                uid: Some("video/a".into()),
                start_ms: 1_000,
                end_ms: 2_000,
                source_id: "s".into(),
            },
        }
    }

    // T-ANK-044: kept, listed in order, read back whole, removed
    #[test]
    fn a_card_kept_comes_back_whole_and_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let later = dir.path().join("later");
        assert!(list(&later).is_empty());
        let first = keep(&later, &card("ご飯", b"one"), false, UNIX_EPOCH).unwrap();
        let second = keep(&later, &card("お茶", b"two"), true, UNIX_EPOCH).unwrap();
        let listed = list(&later);
        assert_eq!(
            listed
                .iter()
                .map(|c| (c.id.as_str(), c.expression.as_str()))
                .collect::<Vec<_>>(),
            [(first.as_str(), "ご飯"), (second.as_str(), "お茶")]
        );
        assert_eq!(listed[0].sentence, "ご飯を 食べた");
        assert_eq!(load(&later, &first).unwrap(), (card("ご飯", b"one"), false));
        assert!(load(&later, &second).unwrap().1, "Add anyway is kept");
        note_problem(&later, &first, "Anki has it").unwrap();
        assert_eq!(list(&later)[0].problem.as_deref(), Some("Anki has it"));
        forget(&later, &first).unwrap();
        assert_eq!(list(&later).len(), 1);
        // the numbers keep growing
        let third = keep(&later, &card("水", b"three"), false, UNIX_EPOCH).unwrap();
        assert!(third > second, "{third} {second}");
        // nothing half written is left, and nothing else is a card
        std::fs::create_dir(later.join(".0000000009.part")).unwrap();
        std::fs::create_dir(later.join("notes")).unwrap();
        assert_eq!(list(&later).len(), 2);
    }

    // T-ANK-044: a damaged folder is listed, never read as a card
    #[test]
    fn a_damaged_card_is_listed_and_never_sent() {
        let dir = tempfile::tempdir().unwrap();
        let later = dir.path();
        let broken = keep(later, &card("壊れ", b"a"), false, UNIX_EPOCH).unwrap();
        std::fs::write(later.join(&broken).join(CARD), b"{").unwrap();
        let escape = keep(later, &card("外", b"b"), false, UNIX_EPOCH).unwrap();
        let path = later.join(&escape).join(CARD);
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("nikomiru_00112233445566ff.png", "../../secret.png");
        std::fs::write(&path, text).unwrap();
        let missing = keep(later, &card("無", b"c"), false, UNIX_EPOCH).unwrap();
        std::fs::remove_file(later.join(&missing).join("nikomiru_00112233445566ff.png")).unwrap();
        let listed = list(later);
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().all(|c| c.damaged && c.expression.is_empty()));
        for id in [&broken, &escape, &missing] {
            assert!(load(later, id).is_err(), "{id}");
        }
        // a name that is not a card's is refused, not followed
        assert!(forget(later, "../x").is_err());
        assert!(load(later, "..").is_err());
        let mut odd = card("名", b"d");
        odd.image = Some(("../x.jpg".into(), Vec::new()));
        assert!(keep(later, &odd, false, UNIX_EPOCH).is_err());
        assert_eq!(list(later).len(), 3, "nothing kept of a refused card");
    }
}
