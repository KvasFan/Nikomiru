// SPDX-License-Identifier: GPL-3.0-only

//! Study statistics (STA-001…STA-005): the time of a video watched counted
//! once (interval coverage), the characters of a cue counted once, the
//! study sessions split by hour, and what the statistics page reads.
//!
//! After Fushi `fushi/lib/src/stats/interval_coverage.dart`,
//! `fushi/lib/src/media/video/video_watch_tracker.dart` and
//! `packages/fushi_audio/lib/src/audiobook/study_clock.dart` (commit
//! `ac8034e`).

use std::collections::{BTreeMap, BTreeSet};

use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

/// The watched intervals of each video (§8 `watch_coverage`).
pub(crate) const COVERAGE: TableDefinition<&str, &[u8]> = TableDefinition::new("watch_coverage");
/// The cues of each video already counted (§8 `cue_credit`).
pub(crate) const CREDIT: TableDefinition<&str, &[u8]> = TableDefinition::new("cue_credit");
/// The study sessions (§8 `study_segments`).
pub(crate) const SEGMENTS: TableDefinition<u128, &[u8]> = TableDefinition::new("study_segments");

/// Merged, sorted intervals `[start, end)` of a video, in ms (STA-001).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Coverage {
    ranges: Vec<(i64, i64)>,
}

impl Coverage {
    /// The intervals, sorted and apart.
    pub fn ranges(&self) -> &[(i64, i64)] {
        &self.ranges
    }

    /// The time covered.
    pub fn total(&self) -> i64 {
        self.ranges.iter().map(|(s, e)| e - s).sum()
    }

    /// Adds `[start, end)`; the parts it did not cover yet, in order.
    pub fn add(&mut self, start: i64, end: i64) -> Vec<(i64, i64)> {
        if end <= start {
            return Vec::new();
        }
        let mut fresh = Vec::new();
        let mut cursor = start;
        for &(s, e) in &self.ranges {
            if e <= cursor {
                continue;
            }
            if s >= end {
                break;
            }
            if s > cursor {
                fresh.push((cursor, s));
            }
            cursor = cursor.max(e);
        }
        if cursor < end {
            fresh.push((cursor, end));
        }
        let (mut lo, mut hi) = (start, end);
        let mut kept: Vec<(i64, i64)> = Vec::with_capacity(self.ranges.len() + 1);
        let mut placed = false;
        for &(s, e) in &self.ranges {
            if e < lo {
                kept.push((s, e));
            } else if s > hi {
                if !placed {
                    kept.push((lo, hi));
                    placed = true;
                }
                kept.push((s, e));
            } else {
                lo = lo.min(s);
                hi = hi.max(e);
            }
        }
        if !placed {
            kept.push((lo, hi));
        }
        self.ranges = kept;
        fresh
    }
}

/// The slack of the playback check, in ms.
pub const SLACK_MS: i64 = 500;

/// Whether the position moving `delta_ms` in `wall_ms` of real time at
/// `speed` is playback (STA-001), not a seek: at most `wall × speed × 1.25 +
/// 500 ms`, and forward.
pub fn is_playback(delta_ms: i64, wall_ms: i64, speed: f64) -> bool {
    delta_ms > 0 && (delta_ms as f64) <= wall_ms as f64 * speed.max(0.01) * 1.25 + SLACK_MS as f64
}

/// The samples of a playing video (STA-001): the position about every
/// second, and at each pause, seek or speed change.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tracker {
    /// The last sample while playing: monotonic ms and position.
    last: Option<(i64, i64)>,
}

impl Tracker {
    /// A sample at monotonic `wall_ms` with the position `pos_ms`, `playing`
    /// from now on; the interval played since the last sample, if it was
    /// playback (a pause counts what was played up to it).
    pub fn sample(
        &mut self,
        wall_ms: i64,
        pos_ms: i64,
        speed: f64,
        playing: bool,
    ) -> Option<(i64, i64)> {
        let previous = std::mem::replace(&mut self.last, playing.then_some((wall_ms, pos_ms)));
        let (then, from) = previous?;
        is_playback(pos_ms - from, wall_ms - then, speed).then_some((from, pos_ms))
    }

    /// The video changed, or stopped: the next sample starts over.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// The cues of a video counted, and the time played in those not yet
/// (STA-002), for one subtitle file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CueCredit {
    /// The subtitle file the indices are of (its source and size): another
    /// file starts over.
    pub key: String,
    /// The cues counted.
    pub counted: BTreeSet<u32>,
    /// The ms played in each cue not counted yet (kept for the session only).
    #[serde(skip)]
    pub played: BTreeMap<u32, i64>,
}

/// The time a cue must be played to count: 1.5 s, or the whole cue if shorter.
pub const CREDIT_MS: i64 = 1_500;

impl CueCredit {
    /// `[from, to)` played: the cues it completes, which count now. `cues`
    /// are `(index, start, end)` in the time of the video.
    pub fn played(&mut self, from: i64, to: i64, cues: &[(u32, i64, i64)]) -> Vec<u32> {
        let mut now = Vec::new();
        for &(i, start, end) in cues {
            if self.counted.contains(&i) || end <= from || start >= to {
                continue;
            }
            let overlap = to.min(end) - from.max(start);
            let ms = self.played.entry(i).or_insert(0);
            *ms += overlap;
            if *ms >= CREDIT_MS.min(end - start) {
                self.played.remove(&i);
                self.counted.insert(i);
                now.push(i);
            }
        }
        now
    }
}

/// A study session (STA-004), within one hour of one day.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Segment {
    /// The video's uid.
    pub media: String,
    /// When it started and when it ended last (Unix ms).
    pub start_ms: i64,
    /// Its last activity (Unix ms).
    pub end_ms: i64,
    /// Its local day, `YYYY-MM-DD`.
    pub date: String,
    /// Its local hour, 0–23.
    pub hour: u8,
    /// The time watched, counted once (STA-001).
    pub duration_ms: i64,
    /// The characters read (STA-002).
    pub chars: u64,
    /// The words looked up.
    pub lookups: u32,
    /// The cards made.
    pub cards: u32,
}

/// The local day and hour of `unix_ms`, `offset_s` east of UTC.
pub fn local_day_hour(unix_ms: i64, offset_s: i32) -> (String, u8) {
    let local = unix_ms.div_euclid(1000) + i64::from(offset_s);
    let days = local.div_euclid(86_400);
    let hour = (local.rem_euclid(86_400) / 3_600) as u8;
    let (y, m, d) = civil(days);
    (format!("{y:04}-{m:02}-{d:02}"), hour)
}

/// The day number (days since 1970-01-01) of a date `YYYY-MM-DD`.
pub fn day_number(date: &str) -> Option<i64> {
    let mut parts = date.splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's `days_from_civil`
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The date `YYYY-MM-DD` of a day number.
pub fn date_of(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// What was studied at a moment (STA-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Activity {
    /// Time watched, counted once.
    pub ms: i64,
    /// Characters read.
    pub chars: u64,
    /// Words looked up.
    pub lookups: u32,
    /// Cards made.
    pub cards: u32,
}

/// A pause longer than this closes a session (STA-004).
pub const PAUSE_MS: i64 = 120_000;

/// The session being recorded (STA-004): what is studied goes into it; a
/// pause over 2 min, another hour or another video starts a new one.
#[derive(Debug, Clone, Default)]
pub struct Clock {
    open: Option<(u128, Segment)>,
}

impl Clock {
    /// The session for `media` at `unix_ms`: the open one, or a new one (the
    /// open one closed and returned, to be saved).
    fn session(
        &mut self,
        media: &str,
        unix_ms: i64,
        offset_s: i32,
        closed: &mut Option<(u128, Segment)>,
    ) -> &mut Segment {
        let (date, hour) = local_day_hour(unix_ms, offset_s);
        let stale = self.open.as_ref().is_some_and(|(_, s)| {
            s.media != media || s.date != date || s.hour != hour || unix_ms - s.end_ms > PAUSE_MS
        });
        if stale {
            *closed = self.open.take();
        }
        let (_, segment) = self.open.get_or_insert_with(|| {
            // the id: the start, then a counter-free tie-breaker of the video
            let id = (u128::from(unix_ms as u64) << 64) | u128::from(fnv(media));
            (
                id,
                Segment {
                    media: media.to_string(),
                    start_ms: unix_ms,
                    end_ms: unix_ms,
                    date,
                    hour,
                    ..Segment::default()
                },
            )
        });
        segment.end_ms = segment.end_ms.max(unix_ms);
        segment
    }

    /// What was studied, added to the session at `unix_ms`; the session it
    /// closed, if any, to save.
    pub fn add(
        &mut self,
        media: &str,
        unix_ms: i64,
        offset_s: i32,
        what: Activity,
    ) -> Option<(u128, Segment)> {
        let mut closed = None;
        let s = self.session(media, unix_ms, offset_s, &mut closed);
        s.duration_ms += what.ms;
        s.chars += what.chars;
        s.lookups += what.lookups;
        s.cards += what.cards;
        closed
    }

    /// The open session, to save as it is now (every minute).
    pub fn current(&self) -> Option<(u128, Segment)> {
        self.open.clone()
    }

    /// Closes the session (another file, the end): it, to save.
    pub fn close(&mut self) -> Option<(u128, Segment)> {
        self.open.take()
    }
}

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The totals of a day (STA-005).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DayTotal {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Time watched.
    pub duration_ms: i64,
    /// Characters read.
    pub chars: u64,
    /// Lookups.
    pub lookups: u32,
    /// Cards.
    pub cards: u32,
}

impl Library {
    /// The watched intervals of a video.
    pub fn coverage(&self, uid: &str) -> Result<Coverage, LibraryError> {
        self.read(|tx| match tx.open_table(COVERAGE) {
            Ok(t) => match db_err!(t.get(uid))? {
                Some(v) => Ok(postcard::from_bytes(v.value())?),
                None => Ok(Coverage::default()),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(Coverage::default()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// The cues of a video counted, for its subtitle file `key`.
    pub fn credit(&self, uid: &str, key: &str) -> Result<CueCredit, LibraryError> {
        let fresh = || CueCredit {
            key: key.to_string(),
            ..CueCredit::default()
        };
        self.read(|tx| match tx.open_table(CREDIT) {
            Ok(t) => match db_err!(t.get(uid))? {
                Some(v) => {
                    let c: CueCredit = postcard::from_bytes(v.value())?;
                    Ok(if c.key == key { c } else { fresh() })
                }
                None => Ok(fresh()),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(fresh()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Saves what a video's watching changed: its coverage, its credits and
    /// the sessions given, in one transaction.
    pub fn save_study(
        &self,
        uid: &str,
        coverage: Option<&Coverage>,
        credit: Option<&CueCredit>,
        segments: &[(u128, Segment)],
    ) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            if let Some(c) = coverage {
                let mut t = db_err!(tx.open_table(COVERAGE))?;
                db_err!(t.insert(uid, postcard::to_stdvec(c)?.as_slice()))?;
            }
            if let Some(c) = credit {
                let mut t = db_err!(tx.open_table(CREDIT))?;
                db_err!(t.insert(uid, postcard::to_stdvec(c)?.as_slice()))?;
            }
            let mut t = db_err!(tx.open_table(SEGMENTS))?;
            for (id, s) in segments {
                // an empty session is not kept
                if s.duration_ms > 0 || s.chars > 0 || s.lookups > 0 || s.cards > 0 {
                    db_err!(t.insert(*id, postcard::to_stdvec(s)?.as_slice()))?;
                }
            }
        }
        db_err!(tx.commit())
    }

    /// Every session, the latest first.
    pub fn segments(&self) -> Result<Vec<(u128, Segment)>, LibraryError> {
        let mut out: Vec<(u128, Segment)> = self.read(|tx| match tx.open_table(SEGMENTS) {
            Ok(t) => {
                let mut out = Vec::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.push((k.value(), postcard::from_bytes(v.value())?));
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(Vec::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })?;
        out.sort_by_key(|(_, s)| std::cmp::Reverse(s.start_ms));
        Ok(out)
    }

    /// Deletes a session (STA-005).
    pub fn delete_segment(&self, id: u128) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(SEGMENTS))?;
            db_err!(t.remove(id))?;
            crate::sync::bury(&tx, &crate::sync::session_key(id))?;
        }
        db_err!(tx.commit())
    }

    /// The totals of each day with a session, by day.
    pub fn day_totals(&self) -> Result<Vec<DayTotal>, LibraryError> {
        let mut days: BTreeMap<String, DayTotal> = BTreeMap::new();
        for (_, s) in self.segments()? {
            let d = days.entry(s.date.clone()).or_insert_with(|| DayTotal {
                date: s.date.clone(),
                ..DayTotal::default()
            });
            d.duration_ms += s.duration_ms;
            d.chars += s.chars;
            d.lookups += s.lookups;
            d.cards += s.cards;
        }
        Ok(days.into_values().collect())
    }

    /// The time and characters of each series, and of the videos of no
    /// series by their file (STA-005), the most studied first.
    pub fn series_totals(&self) -> Result<Vec<(String, i64, u64)>, LibraryError> {
        let videos: BTreeMap<String, crate::VideoRecord> = self.videos()?.into_iter().collect();
        let mut by: BTreeMap<String, (i64, u64)> = BTreeMap::new();
        for (_, s) in self.segments()? {
            let video = videos.get(&s.media);
            let name = video
                .and_then(|v| v.series)
                .and_then(|id| self.series(id).ok().flatten())
                .map(|s| s.title)
                .or_else(|| {
                    video.and_then(|v| {
                        std::path::Path::new(&v.path)
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned())
                    })
                })
                .unwrap_or_else(|| s.media.clone());
            let t = by.entry(name).or_default();
            t.0 += s.duration_ms;
            t.1 += s.chars;
        }
        let mut out: Vec<(String, i64, u64)> =
            by.into_iter().map(|(n, (d, c))| (n, d, c)).collect();
        out.sort_by_key(|(_, d, _)| std::cmp::Reverse(*d));
        Ok(out)
    }

    /// The sessions as CSV (STA-005): a header, then one line each, oldest
    /// first; text fields quoted.
    pub fn sessions_csv(&self) -> Result<String, LibraryError> {
        let videos: BTreeMap<String, crate::VideoRecord> = self.videos()?.into_iter().collect();
        let quote = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
        let mut out =
            String::from("date,hour,start_ms,end_ms,duration_s,chars,lookups,cards,video\n");
        let mut segments = self.segments()?;
        segments.reverse();
        for (_, s) in segments {
            let video = videos
                .get(&s.media)
                .map_or(s.media.as_str(), |v| v.path.as_str());
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                s.date,
                s.hour,
                s.start_ms,
                s.end_ms,
                s.duration_ms / 1000,
                s.chars,
                s.lookups,
                s.cards,
                quote(video)
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-STA-001: Fushi `interval_coverage_test.dart`
    #[test]
    fn coverage_counts_each_interval_once() {
        let mut c = Coverage::default();
        assert_eq!(c.add(0, 1000), [(0, 1000)]);
        assert_eq!(c.add(500, 1500), [(1000, 1500)]);
        assert_eq!(c.add(3000, 4000), [(3000, 4000)]);
        assert_eq!(c.add(0, 5000), [(1500, 3000), (4000, 5000)]);
        assert_eq!(c.ranges(), [(0, 5000)]);
        assert_eq!(c.total(), 5000);
        assert!(c.add(100, 100).is_empty());
        assert!(c.add(10, 20).is_empty());
        let mut c = Coverage::default();
        c.add(10, 20);
        c.add(30, 40);
        c.add(0, 5);
        assert_eq!(c.ranges(), [(0, 5), (10, 20), (30, 40)]);
        c.add(20, 30);
        assert_eq!(c.ranges(), [(0, 5), (10, 40)]);
    }

    // T-STA-001: a seek does not count, nor a pause; speed counts
    #[test]
    fn the_tracker_counts_playback_only() {
        let mut t = Tracker::default();
        assert_eq!(t.sample(0, 10_000, 1.0, true), None);
        assert_eq!(t.sample(1_000, 11_000, 1.0, true), Some((10_000, 11_000)));
        // a seek forward: 30 s in 1 s
        assert_eq!(t.sample(2_000, 41_000, 1.0, true), None);
        assert_eq!(t.sample(3_000, 42_000, 1.0, true), Some((41_000, 42_000)));
        // at 2×
        assert_eq!(t.sample(4_000, 44_000, 2.0, true), Some((42_000, 44_000)));
        // backwards
        assert_eq!(t.sample(5_000, 20_000, 1.0, true), None);
        // a pause: what was played up to it counts, the time it lasts does not
        assert_eq!(t.sample(6_000, 21_000, 1.0, false), Some((20_000, 21_000)));
        assert_eq!(t.sample(60_000, 21_000, 1.0, true), None);
        assert_eq!(t.sample(61_000, 22_000, 1.0, true), Some((21_000, 22_000)));
        assert!(is_playback(1_750, 1_000, 1.0));
        assert!(!is_playback(1_751, 1_000, 1.0));
    }

    // T-STA-002
    #[test]
    fn a_cue_counts_once_after_its_time() {
        let cues = [(0u32, 1_000, 4_000), (1, 4_000, 4_800), (2, 10_000, 12_000)];
        let mut c = CueCredit::default();
        assert!(c.played(1_000, 2_000, &cues).is_empty());
        assert_eq!(c.played(2_000, 3_000, &cues), [0]);
        // a short cue: its whole length
        assert!(c.played(4_000, 4_500, &cues).is_empty());
        assert_eq!(c.played(4_500, 5_000, &cues), [1]);
        // again: nothing
        assert!(c.played(1_000, 5_000, &cues).is_empty());
        // scattered playback adds up
        assert!(c.played(10_000, 10_700, &cues).is_empty());
        assert_eq!(c.played(11_000, 11_900, &cues), [2]);
    }

    // T-STA-004
    #[test]
    fn sessions_split_by_hour_pause_and_video() {
        assert_eq!(local_day_hour(0, 0), ("1970-01-01".to_string(), 0));
        // 2026-10-25 23:30 UTC is 01:30 the next day at UTC+2
        let at = 1_792_970_000_000; // 2026-10-25T23:13:20Z
        assert_eq!(local_day_hour(at, 0), ("2026-10-25".to_string(), 23));
        assert_eq!(local_day_hour(at, 7_200), ("2026-10-26".to_string(), 1));
        for date in ["1970-01-01", "2000-02-29", "2026-10-25", "2026-12-31"] {
            assert_eq!(date_of(day_number(date).unwrap()), date);
        }
        assert_eq!(
            day_number("2026-10-26").unwrap() - day_number("2026-10-25").unwrap(),
            1
        );
        assert_eq!(day_number("2026-13-01"), None);
        let mut clock = Clock::default();
        let start = 1_792_965_600_000; // 22:00:00Z
        assert!(
            clock
                .add(
                    "a",
                    start,
                    0,
                    Activity {
                        ms: 1_000,
                        chars: 10,
                        lookups: 0,
                        cards: 0
                    }
                )
                .is_none()
        );
        assert!(
            clock
                .add(
                    "a",
                    start + 60_000,
                    0,
                    Activity {
                        ms: 59_000,
                        chars: 5,
                        lookups: 1,
                        cards: 0
                    }
                )
                .is_none()
        );
        // a pause over 2 min
        let closed = clock
            .add(
                "a",
                start + 200_000,
                0,
                Activity {
                    ms: 1_000,
                    chars: 0,
                    lookups: 0,
                    cards: 1,
                },
            )
            .unwrap();
        assert_eq!(
            (closed.1.duration_ms, closed.1.chars, closed.1.lookups),
            (60_000, 15, 1)
        );
        // another hour
        let closed = clock
            .add(
                "a",
                start + 3_600_000,
                0,
                Activity {
                    ms: 1_000,
                    chars: 0,
                    lookups: 0,
                    cards: 0,
                },
            )
            .unwrap();
        assert_eq!((closed.1.hour, closed.1.cards), (22, 1));
        // another video
        let closed = clock
            .add(
                "b",
                start + 3_601_000,
                0,
                Activity {
                    ms: 1_000,
                    chars: 0,
                    lookups: 0,
                    cards: 0,
                },
            )
            .unwrap();
        assert_eq!((closed.1.media.as_str(), closed.1.hour), ("a", 23));
        assert_eq!(clock.close().unwrap().1.media, "b");
        assert!(clock.close().is_none());
    }

    // T-STA-005
    #[test]
    fn the_page_reads_days_series_and_csv() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let seg = |media: &str, date: &str, start: i64, ms: i64, chars: u64| Segment {
            media: media.into(),
            start_ms: start,
            end_ms: start + ms,
            date: date.into(),
            hour: 10,
            duration_ms: ms,
            chars,
            ..Segment::default()
        };
        lib.put_video(
            "video/a",
            &crate::VideoRecord {
                path: "/v/Show \"x\".mkv".into(),
                ..Default::default()
            },
        )
        .unwrap();
        lib.save_study(
            "video/a",
            Some(&Coverage {
                ranges: vec![(0, 5)],
            }),
            None,
            &[
                (1, seg("video/a", "2026-10-24", 1_000, 60_000, 100)),
                (2, seg("video/a", "2026-10-25", 2_000, 30_000, 50)),
                (3, seg("video/b", "2026-10-25", 3_000, 90_000, 10)),
                (4, Segment::default()),
            ],
        )
        .unwrap();
        assert_eq!(lib.coverage("video/a").unwrap().total(), 5);
        let days = lib.day_totals().unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(
            (days[1].date.as_str(), days[1].duration_ms, days[1].chars),
            ("2026-10-25", 120_000, 60)
        );
        let series = lib.series_totals().unwrap();
        assert_eq!(series[0], ("Show \"x\"".to_string(), 90_000, 150));
        assert_eq!(series[1].0, "video/b");
        let csv = lib.sessions_csv().unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 4, "{csv}");
        assert_eq!(
            lines[1],
            "2026-10-24,10,1000,61000,60,100,0,0,\"/v/Show \"\"x\"\".mkv\""
        );
        lib.delete_segment(3).unwrap();
        assert_eq!(lib.segments().unwrap().len(), 2);
        // credits are per subtitle file
        let mut c = lib.credit("video/a", "k1").unwrap();
        c.counted.insert(4);
        lib.save_study("video/a", None, Some(&c), &[]).unwrap();
        assert!(lib.credit("video/a", "k1").unwrap().counted.contains(&4));
        assert!(lib.credit("video/a", "k2").unwrap().counted.is_empty());
    }
}
