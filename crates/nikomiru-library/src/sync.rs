// SPDX-License-Identifier: GPL-3.0-only

//! What syncs between devices (BAK-005…BAK-008): the state of one device,
//! merged record by record. Merging is idempotent and commutative, so devices
//! that each write only a file of their own need no locks (BAK-006).

use std::collections::BTreeMap;
use std::io::{Read, Write};

use redb::{ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};

use crate::books::{BOOKS, BookRecord};
use crate::cards::{MINED_CARDS, MinedCard};
use crate::db::{Library, LibraryError, VIDEOS, VideoRecord, db_err};
use crate::stats::{COVERAGE, CREDIT, Coverage, CueCredit, SEGMENTS, Segment};

/// The format of a device's file.
pub const FORMAT: u32 = 1;
/// The largest device file once unpacked.
const UNPACKED_LIMIT: u64 = 256 << 20;
/// How long a deletion is remembered (BAK-008).
pub const TOMBSTONE_MS: i64 = 180 * 24 * 3600 * 1000;

/// What was deleted, and when (Unix ms), by [`session_key`] or [`video_key`].
const TOMBSTONES: TableDefinition<&str, i64> = TableDefinition::new("sync_tombstones");

/// The tombstone of a study session.
pub fn session_key(id: u128) -> String {
    format!("session:{id:032x}")
}

/// The tombstone of a video.
pub fn video_key(uid: &str) -> String {
    format!("video:{uid}")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Notes a deletion, inside the transaction that makes it.
pub(crate) fn bury(tx: &WriteTransaction, key: &str) -> Result<(), LibraryError> {
    let mut t = db_err!(tx.open_table(TOMBSTONES))?;
    db_err!(t.insert(key, now_ms()))?;
    Ok(())
}

/// A video's progress.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VideoState {
    /// Its position.
    pub position_ms: u64,
    /// When it was last played (Unix ms).
    pub last_played_at: Option<i64>,
    /// When it was first watched (Unix ms).
    pub completed_at: Option<i64>,
    /// Its length.
    pub duration_ms: Option<u64>,
}

/// How far a book was read (BAK-005, BOK-005).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BookState {
    /// The characters before the place reached.
    pub read_chars: u64,
    /// The chapter.
    pub chapter: u32,
    /// The paragraph in it.
    pub paragraph: u32,
    /// When it was last read (Unix ms).
    pub last_read_at: Option<i64>,
}

/// What a device knows.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceState {
    /// The format of the file.
    pub format: u32,
    /// The videos, by uid.
    pub videos: BTreeMap<String, VideoState>,
    /// The watched intervals, by uid.
    pub coverage: BTreeMap<String, Coverage>,
    /// The cues counted, by uid.
    pub credit: BTreeMap<String, CueCredit>,
    /// The study sessions, by id in hexadecimal.
    pub sessions: BTreeMap<String, Segment>,
    /// The cards made, by note id.
    pub cards: BTreeMap<u64, MinedCard>,
    /// What was deleted, and when.
    pub deleted: BTreeMap<String, i64>,
    /// How far each book was read, by uid: older files have none.
    #[serde(default)]
    pub books: BTreeMap<String, BookState>,
}

fn merge_video(into: &mut VideoState, from: &VideoState) {
    // the one played last; on a tie the further one, so that the order of
    // the devices changes nothing (BAK-007)
    let theirs = (from.last_played_at, from.position_ms);
    if theirs > (into.last_played_at, into.position_ms) {
        into.position_ms = from.position_ms;
        into.last_played_at = from.last_played_at;
    }
    into.completed_at = match (into.completed_at, from.completed_at) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    into.duration_ms = into.duration_ms.max(from.duration_ms);
}

fn merge_credit(into: &mut CueCredit, from: &CueCredit) {
    if into.key == from.key {
        into.counted.extend(from.counted.iter().copied());
    } else if from.key > into.key {
        // another subtitle file: one of them, the same on every device
        into.key.clone_from(&from.key);
        into.counted.clone_from(&from.counted);
    }
}

fn merge_session(into: &mut Segment, from: &Segment) {
    // a session still growing on the device that has it
    if (from.end_ms, from.duration_ms, from.chars) > (into.end_ms, into.duration_ms, into.chars) {
        *into = from.clone();
    }
}

fn merge_book(into: &mut BookState, from: &BookState) {
    // the one read last; on a tie the further one, as for videos (BAK-007)
    let key = |b: &BookState| (b.last_read_at, b.read_chars, b.chapter, b.paragraph);
    if key(from) > key(into) {
        *into = from.clone();
    }
}

impl DeviceState {
    /// Takes in what `other` knows (BAK-007).
    pub fn merge(&mut self, other: &DeviceState) {
        for (uid, book) in &other.books {
            match self.books.get_mut(uid) {
                Some(mine) => merge_book(mine, book),
                None => {
                    self.books.insert(uid.clone(), book.clone());
                }
            }
        }
        for (uid, video) in &other.videos {
            match self.videos.get_mut(uid) {
                Some(mine) => merge_video(mine, video),
                None => {
                    self.videos.insert(uid.clone(), video.clone());
                }
            }
        }
        for (uid, coverage) in &other.coverage {
            let mine = self.coverage.entry(uid.clone()).or_default();
            for (start, end) in coverage.ranges() {
                mine.add(*start, *end);
            }
        }
        for (uid, credit) in &other.credit {
            match self.credit.get_mut(uid) {
                Some(mine) => merge_credit(mine, credit),
                None => {
                    self.credit.insert(uid.clone(), credit.clone());
                }
            }
        }
        for (id, session) in &other.sessions {
            match self.sessions.get_mut(id) {
                Some(mine) => merge_session(mine, session),
                None => {
                    self.sessions.insert(id.clone(), session.clone());
                }
            }
        }
        for (id, card) in &other.cards {
            match self.cards.get_mut(id) {
                Some(mine)
                    if (mine.created_at, &mine.source_id) < (card.created_at, &card.source_id) =>
                {
                    *mine = card.clone();
                }
                Some(_) => {}
                None => {
                    self.cards.insert(*id, card.clone());
                }
            }
        }
        for (key, at) in &other.deleted {
            let mine = self.deleted.entry(key.clone()).or_insert(*at);
            *mine = (*mine).max(*at);
        }
        self.sessions
            .retain(|id, _| !self.deleted.contains_key(&format!("session:{id}")));
    }

    /// The tombstones older than `now` allows go (BAK-008).
    pub fn forget_old(&mut self, now: i64) {
        self.deleted.retain(|_, at| now - *at < TOMBSTONE_MS);
    }

    /// Its file: compressed JSON, so that a later format can read it.
    pub fn encode(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        out.write_all(&serde_json::to_vec(self)?)?;
        out.finish()
    }

    /// A state from its file; what cannot be read is an error, and leaves the
    /// library alone (BAK-006).
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut text = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(UNPACKED_LIMIT + 1)
            .read_to_end(&mut text)
            .map_err(|_| DecodeError::Damaged)?;
        if text.len() as u64 > UNPACKED_LIMIT {
            return Err(DecodeError::Damaged);
        }
        // the format first: a later one may not parse as this one
        #[derive(Deserialize)]
        struct Header {
            format: u32,
        }
        let header: Header = serde_json::from_slice(&text).map_err(|_| DecodeError::Damaged)?;
        if header.format > FORMAT {
            return Err(DecodeError::Newer(header.format));
        }
        serde_json::from_slice(&text).map_err(|_| DecodeError::Damaged)
    }
}

/// Why a device's file was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// It cannot be read.
    #[error("a device's file is damaged")]
    Damaged,
    /// It was written by a newer Nikomiru.
    #[error("a device's file has format {0}")]
    Newer(u32),
}

fn collect<K, V>(
    tx: &redb::ReadTransaction,
    table: TableDefinition<K, &[u8]>,
    mut each: impl FnMut(K::SelfType<'_>, V),
) -> Result<(), LibraryError>
where
    K: redb::Key + 'static,
    V: serde::de::DeserializeOwned,
{
    match tx.open_table(table) {
        Ok(t) => {
            for row in db_err!(t.iter())? {
                let (k, v) = db_err!(row)?;
                each(k.value(), postcard::from_bytes(v.value())?);
            }
            Ok(())
        }
        Err(redb::TableError::TableDoesNotExist(_)) => Ok(()),
        Err(e) => Err(LibraryError::Db(e.into())),
    }
}

impl Library {
    /// What this device knows (BAK-005): never settings, paths or series.
    pub fn export_state(&self, now: i64) -> Result<DeviceState, LibraryError> {
        let mut state = DeviceState {
            format: FORMAT,
            ..DeviceState::default()
        };
        self.read(|tx| {
            collect::<_, VideoRecord>(tx, VIDEOS, |uid: &str, v| {
                if v.last_played_at.is_some() || v.completed_at.is_some() {
                    state.videos.insert(
                        uid.to_string(),
                        VideoState {
                            position_ms: v.last_position_ms,
                            last_played_at: v.last_played_at,
                            completed_at: v.completed_at,
                            duration_ms: v.duration_ms,
                        },
                    );
                }
            })?;
            collect::<_, Coverage>(tx, COVERAGE, |uid: &str, c| {
                state.coverage.insert(uid.to_string(), c);
            })?;
            collect::<_, CueCredit>(tx, CREDIT, |uid: &str, c| {
                state.credit.insert(uid.to_string(), c);
            })?;
            collect::<_, Segment>(tx, SEGMENTS, |id: u128, s| {
                state.sessions.insert(format!("{id:032x}"), s);
            })?;
            collect::<_, MinedCard>(tx, MINED_CARDS, |id: u64, c| {
                state.cards.insert(id, c);
            })?;
            collect::<_, BookRecord>(tx, BOOKS, |uid: &str, b| {
                if b.last_read_at.is_some() {
                    state.books.insert(
                        uid.to_string(),
                        BookState {
                            read_chars: b.read_chars,
                            chapter: b.chapter,
                            paragraph: b.paragraph,
                            last_read_at: b.last_read_at,
                        },
                    );
                }
            })?;
            match tx.open_table(TOMBSTONES) {
                Ok(t) => {
                    for row in db_err!(t.iter())? {
                        let (k, v) = db_err!(row)?;
                        state.deleted.insert(k.value().to_string(), v.value());
                    }
                    Ok(())
                }
                Err(redb::TableError::TableDoesNotExist(_)) => Ok(()),
                Err(e) => Err(LibraryError::Db(e.into())),
            }
        })?;
        state.forget_old(now);
        Ok(state)
    }

    /// Writes a merged state, in one transaction (BAK-009). A video this
    /// computer does not have is left out: its record comes from a scan, and
    /// the next sync fills it in.
    pub fn apply_state(&self, state: &DeviceState) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut videos = db_err!(tx.open_table(VIDEOS))?;
            let mut changed = Vec::new();
            for row in db_err!(videos.iter())? {
                let (uid, value) = db_err!(row)?;
                let uid = uid.value().to_string();
                let mut record: VideoRecord = postcard::from_bytes(value.value())?;
                let before = record.clone();
                if let Some(v) = state.videos.get(&uid) {
                    record.last_position_ms = v.position_ms;
                    record.last_played_at = v.last_played_at;
                    record.completed_at = v.completed_at;
                    record.duration_ms = record.duration_ms.or(v.duration_ms);
                }
                if state.deleted.contains_key(&video_key(&uid)) {
                    record.removed = true;
                }
                if record != before {
                    changed.push((uid, record));
                }
            }
            for (uid, record) in changed {
                db_err!(videos.insert(uid.as_str(), postcard::to_stdvec(&record)?.as_slice()))?;
            }
            // a book this computer does not have is left out, as a video is
            let mut books = db_err!(tx.open_table(BOOKS))?;
            let mut read = Vec::new();
            for row in db_err!(books.iter())? {
                let (uid, value) = db_err!(row)?;
                let uid = uid.value().to_string();
                if let Some(b) = state.books.get(&uid) {
                    let mut record: BookRecord = postcard::from_bytes(value.value())?;
                    record.read_chars = b.read_chars;
                    record.chapter = b.chapter;
                    record.paragraph = b.paragraph;
                    record.last_read_at = b.last_read_at;
                    read.push((uid, record));
                }
            }
            for (uid, record) in read {
                db_err!(books.insert(uid.as_str(), postcard::to_stdvec(&record)?.as_slice()))?;
            }
            let mut coverage = db_err!(tx.open_table(COVERAGE))?;
            for (uid, c) in &state.coverage {
                db_err!(coverage.insert(uid.as_str(), postcard::to_stdvec(c)?.as_slice()))?;
            }
            let mut credit = db_err!(tx.open_table(CREDIT))?;
            for (uid, c) in &state.credit {
                db_err!(credit.insert(uid.as_str(), postcard::to_stdvec(c)?.as_slice()))?;
            }
            let mut sessions = db_err!(tx.open_table(SEGMENTS))?;
            for (id, s) in &state.sessions {
                if let Ok(id) = u128::from_str_radix(id, 16) {
                    db_err!(sessions.insert(id, postcard::to_stdvec(s)?.as_slice()))?;
                }
            }
            let mut cards = db_err!(tx.open_table(MINED_CARDS))?;
            for (id, c) in &state.cards {
                db_err!(cards.insert(*id, postcard::to_stdvec(c)?.as_slice()))?;
            }
            let mut tombstones = db_err!(tx.open_table(TOMBSTONES))?;
            for (key, at) in &state.deleted {
                if let Some(id) = key
                    .strip_prefix("session:")
                    .and_then(|id| u128::from_str_radix(id, 16).ok())
                {
                    db_err!(sessions.remove(id))?;
                }
                db_err!(tombstones.insert(key.as_str(), *at))?;
            }
        }
        db_err!(tx.commit())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn video() -> impl Strategy<Value = VideoState> {
        (
            0..1000u64,
            proptest::option::of(0..50i64),
            proptest::option::of(0..50i64),
            proptest::option::of(0..1000u64),
        )
            .prop_map(|(position_ms, last_played_at, completed_at, duration_ms)| {
                VideoState {
                    position_ms,
                    last_played_at,
                    completed_at,
                    duration_ms,
                }
            })
    }

    fn session() -> impl Strategy<Value = Segment> {
        (0..50i64, 0..50i64).prop_map(|(end_ms, duration_ms)| Segment {
            end_ms,
            duration_ms,
            ..Segment::default()
        })
    }

    fn card() -> impl Strategy<Value = MinedCard> {
        (0..20i64, "[ab]{0,2}").prop_map(|(created_at, source_id)| MinedCard {
            expression: String::new(),
            reading: String::new(),
            sentence: String::new(),
            video_uid: String::new(),
            start_ms: 0,
            end_ms: 0,
            source_id,
            created_at,
        })
    }

    fn book() -> impl Strategy<Value = BookState> {
        (0..50u64, 0..3u32, 0..3u32, proptest::option::of(0..9i64)).prop_map(
            |(read_chars, chapter, paragraph, last_read_at)| BookState {
                read_chars,
                chapter,
                paragraph,
                last_read_at,
            },
        )
    }

    fn state() -> impl Strategy<Value = DeviceState> {
        let names = prop::sample::select(vec!["a", "b", "c"]);
        (
            prop::collection::btree_map(names.clone(), video(), 0..3),
            prop::collection::btree_map(
                names.clone(),
                prop::collection::vec((0..100i64, 1..50i64), 0..4),
                0..3,
            ),
            prop::collection::btree_map(
                names.clone(),
                ("[xy]", prop::collection::btree_set(0..8u32, 0..4)),
                0..3,
            ),
            prop::collection::btree_map(names.clone(), session(), 0..3),
            prop::collection::btree_map(0..3u64, card(), 0..3),
            prop::collection::btree_map(names.clone(), 0..50i64, 0..2),
            prop::collection::btree_map(names, book(), 0..3),
        )
            .prop_map(
                |(videos, coverage, credit, sessions, cards, deleted, books)| DeviceState {
                    books: books.into_iter().map(|(k, b)| (k.to_string(), b)).collect(),
                    format: FORMAT,
                    videos: videos
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v))
                        .collect(),
                    coverage: coverage
                        .into_iter()
                        .map(|(k, ranges)| {
                            let mut c = Coverage::default();
                            for (start, length) in ranges {
                                c.add(start, start + length);
                            }
                            (k.to_string(), c)
                        })
                        .collect(),
                    credit: credit
                        .into_iter()
                        .map(|(k, (key, counted))| {
                            (
                                k.to_string(),
                                CueCredit {
                                    key,
                                    counted,
                                    ..CueCredit::default()
                                },
                            )
                        })
                        .collect(),
                    sessions: sessions
                        .into_iter()
                        .map(|(k, s)| (k.to_string(), s))
                        .collect(),
                    cards,
                    deleted: deleted
                        .into_iter()
                        .map(|(k, at)| (format!("session:{k}"), at))
                        .collect(),
                },
            )
    }

    fn merged(a: &DeviceState, b: &DeviceState) -> DeviceState {
        let mut out = a.clone();
        out.merge(b);
        out
    }

    proptest! {
        // T-BAK-007: merging twice changes nothing
        #[test]
        fn merging_is_idempotent(a in state(), b in state()) {
            let once = merged(&a, &b);
            prop_assert_eq!(merged(&once, &b), once);
        }

        // T-BAK-007: the order of the devices changes nothing
        #[test]
        fn merging_is_commutative(a in state(), b in state()) {
            prop_assert_eq!(merged(&a, &b), merged(&b, &a));
        }

        // T-BAK-007
        #[test]
        fn merging_is_associative(a in state(), b in state(), c in state()) {
            prop_assert_eq!(merged(&merged(&a, &b), &c), merged(&a, &merged(&b, &c)));
        }

        // T-BAK-010: what a server sends is untrusted
        #[test]
        fn any_bytes_are_refused_without_a_panic(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
            let _ = DeviceState::decode(&bytes);
        }
    }

    fn video_at(position_ms: u64, at: i64) -> VideoState {
        VideoState {
            position_ms,
            last_played_at: Some(at),
            ..VideoState::default()
        }
    }

    // T-BAK-007
    #[test]
    fn the_video_played_last_wins_and_the_first_watched_stays() {
        let mut a = VideoState {
            completed_at: Some(50),
            ..video_at(10, 100)
        };
        let b = VideoState {
            completed_at: Some(20),
            ..video_at(20, 200)
        };
        merge_video(&mut a, &b);
        assert_eq!((a.position_ms, a.completed_at), (20, Some(20)));
        merge_video(&mut a, &video_at(30, 150));
        assert_eq!(a.position_ms, 20);
    }

    // T-BAK-006
    #[test]
    fn a_file_of_another_format_is_not_read() {
        let state = DeviceState {
            format: FORMAT,
            ..DeviceState::default()
        };
        let bytes = state.encode().unwrap();
        assert_eq!(DeviceState::decode(&bytes), Ok(state.clone()));
        let newer = DeviceState {
            format: FORMAT + 1,
            ..state
        };
        assert_eq!(
            DeviceState::decode(&newer.encode().unwrap()),
            Err(DecodeError::Newer(FORMAT + 1))
        );
        assert_eq!(DeviceState::decode(b"junk"), Err(DecodeError::Damaged));
    }

    fn open(dir: &tempfile::TempDir, name: &str) -> Library {
        Library::open(&dir.path().join(name)).unwrap()
    }

    fn played(uid: &str, position: u64, at: i64) -> (String, VideoRecord) {
        (
            uid.to_string(),
            VideoRecord {
                path: format!("/v/{uid}"),
                last_position_ms: position,
                last_played_at: Some(at),
                ..VideoRecord::default()
            },
        )
    }

    // T-BAK-005, T-BAK-007: two libraries meet through their states
    #[test]
    fn two_devices_end_up_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (open(&dir, "a.redb"), open(&dir, "b.redb"));
        let (uid, record) = played("video/ep1", 1000, 100);
        a.put_video(&uid, &record).unwrap();
        let (uid, record) = played("video/ep1", 5000, 200);
        b.put_video(&uid, &record).unwrap();
        let segment = Segment {
            media: "video/ep1".into(),
            duration_ms: 10,
            ..Segment::default()
        };
        a.save_study("video/ep1", None, None, &[(7, segment.clone())])
            .unwrap();
        let mut merged = a.export_state(1000).unwrap();
        merged.merge(&b.export_state(1000).unwrap());
        a.apply_state(&merged).unwrap();
        b.apply_state(&merged).unwrap();
        assert_eq!(
            a.video("video/ep1").unwrap().unwrap().last_position_ms,
            5000
        );
        // its path stays this computer's
        assert_eq!(a.video("video/ep1").unwrap().unwrap().path, "/v/video/ep1");
        assert_eq!(b.segments().unwrap(), vec![(7, segment)]);
        assert_eq!(a.export_state(1000).unwrap(), b.export_state(1000).unwrap());
    }

    // T-BAK-005: a video only the other has makes no row here
    #[test]
    fn a_video_of_another_computer_makes_no_record() {
        let dir = tempfile::tempdir().unwrap();
        let a = open(&dir, "a.redb");
        let mut state = DeviceState::default();
        state.videos.insert("video/other".into(), video_at(1, 1));
        a.apply_state(&state).unwrap();
        assert_eq!(a.video("video/other").unwrap(), None);
    }

    // T-BAK-005: a book goes where it was read last, here too
    #[test]
    fn a_book_keeps_its_place_across_devices() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (open(&dir, "a.redb"), open(&dir, "b.redb"));
        let record = |paragraph, at| BookRecord {
            path: format!("/{paragraph}"),
            paragraph,
            read_chars: u64::from(paragraph),
            last_read_at: Some(at),
            ..BookRecord::default()
        };
        a.put_book("book/x", &record(1, 100)).unwrap();
        b.put_book("book/x", &record(7, 200)).unwrap();
        let mut merged = a.export_state(1000).unwrap();
        merged.merge(&b.export_state(1000).unwrap());
        a.apply_state(&merged).unwrap();
        let book = a.book("book/x").unwrap().unwrap();
        assert_eq!(
            (book.paragraph, book.path.as_str()),
            (7, "/1"),
            "its file stays"
        );
        // a file from before books reads as having none
        let old = br#"{"format":1,"videos":{},"coverage":{},"credit":{},"sessions":{},"cards":{},"deleted":{}}"#;
        let mut packed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        packed.write_all(old).unwrap();
        let state = DeviceState::decode(&packed.finish().unwrap()).unwrap();
        assert!(state.books.is_empty());
    }

    // T-BAK-008
    #[test]
    fn a_deleted_session_does_not_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (open(&dir, "a.redb"), open(&dir, "b.redb"));
        let segment = Segment {
            duration_ms: 10,
            ..Segment::default()
        };
        a.save_study("v", None, None, &[(7, segment.clone())])
            .unwrap();
        b.save_study("v", None, None, &[(7, segment)]).unwrap();
        a.delete_segment(7).unwrap();
        let now = now_ms();
        let mut merged = a.export_state(now).unwrap();
        merged.merge(&b.export_state(now).unwrap());
        assert!(merged.sessions.is_empty());
        a.apply_state(&merged).unwrap();
        b.apply_state(&merged).unwrap();
        assert!(a.segments().unwrap().is_empty() && b.segments().unwrap().is_empty());
        // and a deletion is forgotten in time
        assert!(
            a.export_state(now + TOMBSTONE_MS)
                .unwrap()
                .deleted
                .is_empty()
        );
    }

    // T-BAK-008
    #[test]
    fn a_removed_video_stays_removed_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (open(&dir, "a.redb"), open(&dir, "b.redb"));
        for lib in [&a, &b] {
            let (uid, record) = played("video/ep1", 1, 1);
            lib.put_video(&uid, &record).unwrap();
        }
        a.remove_video("video/ep1").unwrap();
        let mut merged = b.export_state(now_ms()).unwrap();
        merged.merge(&a.export_state(now_ms()).unwrap());
        b.apply_state(&merged).unwrap();
        assert!(b.video("video/ep1").unwrap().unwrap().removed);
    }
}
