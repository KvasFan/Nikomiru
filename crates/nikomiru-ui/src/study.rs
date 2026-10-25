// SPDX-License-Identifier: GPL-3.0-only

//! What the watching of the current video adds to the statistics
//! (STA-001…STA-004): the position sampled about every second, the time
//! played counted once, the cues counted once, the lookups and the cards,
//! all going into the study session; saved every minute and when the video
//! changes or Nikomiru closes.

use nikomiru_library::stats::{Activity, Clock, Coverage, CueCredit, Segment, Tracker};

/// How often the position is sampled while playing (STA-001).
pub const SAMPLE_MS: i64 = 1_000;

/// How often what changed is saved (STA-004).
pub const SAVE_MS: i64 = 60_000;

/// A cue as the credits see it: its index, start and end in the video's
/// time, and its characters (STA-003).
pub type CueSpan = (u32, i64, i64, u64);

/// What to write for a video.
#[derive(Debug, Clone, PartialEq)]
pub struct Save {
    /// The video.
    pub uid: String,
    /// Its watched intervals.
    pub coverage: Coverage,
    /// Its cues counted, for its subtitle file.
    pub credit: Option<CueCredit>,
    /// Sessions, closed or as they are now.
    pub segments: Vec<(u128, Segment)>,
}

/// The study of the current video.
#[derive(Debug, Default)]
pub struct Study {
    uid: Option<String>,
    tracker: Tracker,
    coverage: Coverage,
    credit: Option<CueCredit>,
    clock: Clock,
    /// Sessions closed and not saved yet.
    closed: Vec<(u128, Segment)>,
    /// Something changed since the last save.
    dirty: bool,
    last_sample: Option<i64>,
    last_save: Option<i64>,
}

/// When something happens: monotonic ms, Unix ms, and the local offset.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    /// Monotonic ms.
    pub mono_ms: i64,
    /// Unix ms.
    pub unix_ms: i64,
    /// Seconds east of UTC.
    pub offset_s: i32,
}

impl Study {
    /// Another video (`None`: one the library does not keep): what the last
    /// one leaves to save.
    pub fn start(&mut self, uid: Option<String>, coverage: Coverage) -> Option<Save> {
        let save = self.finish();
        self.uid = uid;
        self.coverage = coverage;
        self.credit = None;
        self.tracker.reset();
        self.last_sample = None;
        save
    }

    /// The subtitles changed: the cues counted for them.
    pub fn credits(&mut self, credit: CueCredit) {
        self.credit = Some(credit);
    }

    /// The key the credits of `source` with `cues` cues are kept under.
    pub fn credit_key(source: &str, cues: usize) -> String {
        format!("{source}#{cues}")
    }

    /// Whether a sample is due: about every second while playing.
    pub fn due_sample(&self, mono_ms: i64) -> bool {
        self.last_sample.is_none_or(|t| mono_ms - t >= SAMPLE_MS)
    }

    fn add(&mut self, at: Moment, what: Activity) {
        let Some(uid) = self.uid.clone() else {
            return;
        };
        if let Some(closed) = self.clock.add(&uid, at.unix_ms, at.offset_s, what) {
            self.closed.push(closed);
        }
        // the minute to the next save starts with the first thing to save
        self.last_save.get_or_insert(at.mono_ms);
        self.dirty = true;
    }

    /// The position `pos_ms` at `speed`, `playing` from now on (STA-001,
    /// STA-002): the time played since the last sample counted once, the
    /// cues it completes counted; `cues(from, to)` gives the cues there.
    pub fn sample(
        &mut self,
        at: Moment,
        pos_ms: i64,
        speed: f64,
        playing: bool,
        cues: &dyn Fn(i64, i64) -> Vec<CueSpan>,
    ) {
        self.last_sample = Some(at.mono_ms);
        let Some((from, to)) = self.tracker.sample(at.mono_ms, pos_ms, speed, playing) else {
            return;
        };
        let fresh: i64 = self.coverage.add(from, to).iter().map(|(s, e)| e - s).sum();
        let mut chars = 0;
        if let Some(credit) = self.credit.as_mut() {
            let spans = cues(from, to);
            let found: Vec<(u32, i64, i64)> = spans.iter().map(|&(i, s, e, _)| (i, s, e)).collect();
            for i in credit.played(from, to, &found) {
                chars += spans.iter().find(|(j, ..)| *j == i).map_or(0, |(.., n)| *n);
            }
        }
        if fresh > 0 || chars > 0 {
            self.add(
                at,
                Activity {
                    ms: fresh,
                    chars,
                    ..Activity::default()
                },
            );
        }
    }

    /// A word looked up.
    pub fn lookup(&mut self, at: Moment) {
        self.add(
            at,
            Activity {
                lookups: 1,
                ..Activity::default()
            },
        );
    }

    /// A card made.
    pub fn card(&mut self, at: Moment) {
        self.add(
            at,
            Activity {
                cards: 1,
                ..Activity::default()
            },
        );
    }

    fn save(&mut self, close: bool) -> Option<Save> {
        let uid = self.uid.clone()?;
        if !self.dirty && self.closed.is_empty() {
            return None;
        }
        self.dirty = false;
        let mut segments = std::mem::take(&mut self.closed);
        let open = if close {
            self.clock.close()
        } else {
            self.clock.current()
        };
        segments.extend(open);
        Some(Save {
            uid,
            coverage: self.coverage.clone(),
            credit: self.credit.clone(),
            segments,
        })
    }

    /// What to save, once a minute (STA-004): the open session as it is.
    pub fn due_save(&mut self, mono_ms: i64) -> Option<Save> {
        if self.last_save.is_none_or(|last| mono_ms - last < SAVE_MS) {
            return None;
        }
        self.last_save = Some(mono_ms);
        self.save(false)
    }

    /// What to save now, the session left open (the statistics page shows
    /// it).
    pub fn save_now(&mut self) -> Option<Save> {
        self.save(false)
    }

    /// The video ends here (another one, or Nikomiru closes): what to save,
    /// the session closed.
    pub fn finish(&mut self) -> Option<Save> {
        let save = self.save(true);
        self.clock.close();
        save
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mono_ms: i64) -> Moment {
        Moment {
            mono_ms,
            unix_ms: 1_792_965_600_000 + mono_ms,
            offset_s: 0,
        }
    }

    // T-STA-001, T-STA-002, T-STA-004
    #[test]
    fn watching_fills_the_session_once() {
        let mut s = Study::default();
        assert!(
            s.start(Some("video/a".into()), Coverage::default())
                .is_none()
        );
        s.credits(CueCredit::default());
        let cues = |from: i64, to: i64| -> Vec<CueSpan> {
            [(0u32, 0, 2_000, 5u64), (1, 2_000, 6_000, 7)]
                .into_iter()
                .filter(|&(_, a, b, _)| a < to && b > from)
                .collect()
        };
        for t in 0..=6 {
            s.sample(at(t * 1_000), t * 1_000, 1.0, true, &cues);
        }
        // a seek back: the same interval again counts no time and no cue
        s.sample(at(7_000), 0, 1.0, true, &cues);
        for t in 1..=6 {
            s.sample(at(7_000 + t * 1_000), t * 1_000, 1.0, true, &cues);
        }
        s.lookup(at(14_000));
        s.card(at(14_500));
        let save = s.due_save(70_000).unwrap();
        assert_eq!(save.uid, "video/a");
        assert_eq!(save.coverage.total(), 6_000);
        let (_, seg) = &save.segments[0];
        assert_eq!(
            (seg.duration_ms, seg.chars, seg.lookups, seg.cards),
            (6_000, 12, 1, 1)
        );
        // nothing new: nothing to save
        assert!(s.due_save(140_000).is_none());
        // another video: the session closes
        let save = s.start(Some("video/b".into()), Coverage::default());
        assert!(save.is_none(), "saved a minute ago and nothing since");
        s.sample(at(200_000), 0, 1.0, true, &cues);
        s.sample(at(201_000), 1_000, 1.0, true, &cues);
        let save = s.finish().unwrap();
        assert_eq!(
            (save.uid.as_str(), save.segments[0].1.duration_ms),
            ("video/b", 1_000)
        );
        assert!(s.finish().is_none());
    }
}
