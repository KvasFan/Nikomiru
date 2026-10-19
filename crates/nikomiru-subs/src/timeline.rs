// SPDX-License-Identifier: GPL-3.0-only

//! Cues against the playback position: the active cues, the offset, navigation
//! and the list that follows playback (SUB-009, SUB-011…SUB-013, SUB-015,
//! SUB-016). Positions are player positions in milliseconds; a cue time is
//! `pos − delay_ms`.

use crate::cue::Cue;

/// Navigation lands this much before the start of the cue (SUB-012).
pub const LEAD_IN_MS: i64 = 180;

/// From the keyboard, a previous cue farther than this is reached with a time
/// seek instead (SUB-012, BUG-3339).
pub const KEYBOARD_REACH_MS: i64 = 10_000;

/// Z/X step of the offset (SUB-011).
pub const DELAY_STEP_MS: i64 = 100;

/// Where a navigation command takes the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// To this position.
    To(i64),
    /// A relative time seek of this many seconds (no usable cue).
    Seek(i64),
}

/// The cues of a track with its offset.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    cues: Vec<Cue>,
    /// Running maximum of `end_ms`, to find long cues that are still active.
    max_end: Vec<i64>,
    /// Positive: the subtitles appear later (SUB-011).
    pub delay_ms: i64,
}

impl Timeline {
    /// A timeline over cues sorted by start (as the parsers return them).
    pub fn new(cues: Vec<Cue>) -> Self {
        let mut max = i64::MIN;
        let max_end = cues
            .iter()
            .map(|c| {
                max = max.max(c.end_ms);
                max
            })
            .collect();
        Self {
            cues,
            max_end,
            delay_ms: 0,
        }
    }

    /// The cues.
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    /// Whether there are no cues.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    fn cue_time(&self, pos_ms: i64) -> i64 {
        pos_ms - self.delay_ms
    }

    /// Index of the last cue that starts at or before cue time `t` (*floor*).
    fn floor(&self, t: i64) -> Option<usize> {
        self.cues
            .partition_point(|c| c.start_ms <= t)
            .checked_sub(1)
    }

    /// The cues active at `pos_ms`, in drawing order: layer, then file order
    /// (SUB-009).
    pub fn active(&self, pos_ms: i64) -> Vec<usize> {
        let t = self.cue_time(pos_ms);
        let Some(last) = self.floor(t) else {
            return Vec::new();
        };
        let mut out: Vec<usize> = (0..=last)
            .rev()
            .take_while(|&i| self.max_end[i] > t)
            .filter(|&i| self.cues[i].contains(t))
            .collect();
        out.sort_by_key(|&i| (self.cues[i].layer, self.cues[i].order));
        out
    }

    /// The player position of the start of cue `i`.
    pub fn start_pos(&self, i: usize) -> i64 {
        self.cues[i].start_ms + self.delay_ms
    }

    /// Where navigation to cue `i` lands: `LEAD_IN_MS` before it, never before
    /// the start of the cue that precedes it, never before 0 (BUG-259).
    fn landing(&self, i: usize) -> i64 {
        let target = self.start_pos(i) - LEAD_IN_MS;
        let floor = i.checked_sub(1).map_or(0, |p| self.start_pos(p));
        target.max(floor).max(0)
    }

    /// "Previous cue" (SUB-012). Inside a cue it goes to the cue before; in a
    /// gap, to the cue that just ended. From the keyboard, a cue farther than
    /// [`KEYBOARD_REACH_MS`] becomes a time seek of `seek_seconds`.
    pub fn previous(&self, pos_ms: i64, keyboard: bool, seek_seconds: i64) -> Jump {
        let t = self.cue_time(pos_ms);
        let target = self.floor(t).and_then(|f| {
            if self.cues[f].contains(t) {
                f.checked_sub(1)
            } else {
                Some(f)
            }
        });
        match target {
            Some(i) if !(keyboard && pos_ms - self.start_pos(i) > KEYBOARD_REACH_MS) => {
                Jump::To(self.landing(i))
            }
            _ => Jump::Seek(-seek_seconds),
        }
    }

    /// "Next cue" (SUB-012): the first cue that starts after the position.
    pub fn next(&self, pos_ms: i64, seek_seconds: i64) -> Jump {
        let t = self.cue_time(pos_ms);
        let i = self.cues.partition_point(|c| c.start_ms <= t);
        if i < self.cues.len() {
            Jump::To(self.landing(i))
        } else {
            Jump::Seek(seek_seconds)
        }
    }

    /// "Replay cue" (SUB-013): the start and end positions of the current cue
    /// (the active one, or the one that just ended).
    pub fn replay(&self, pos_ms: i64) -> Option<(i64, i64)> {
        let t = self.cue_time(pos_ms);
        let i = self
            .active(pos_ms)
            .last()
            .copied()
            .or_else(|| self.floor(t))?;
        Some((self.start_pos(i), self.cues[i].end_ms + self.delay_ms))
    }

    /// Z/X (SUB-011): moves the offset by `steps` × [`DELAY_STEP_MS`].
    pub fn nudge(&mut self, steps: i64) -> i64 {
        self.delay_ms += steps * DELAY_STEP_MS;
        self.delay_ms
    }

    /// Ctrl+Shift+←/→ (SUB-016): the previous (`forward = false`) or next cue
    /// starts at the current position; returns the new offset.
    pub fn align(&mut self, pos_ms: i64, forward: bool) -> Option<i64> {
        let t = self.cue_time(pos_ms);
        let i = if forward {
            let i = self.cues.partition_point(|c| c.start_ms <= t);
            (i < self.cues.len()).then_some(i)?
        } else {
            self.floor(t)?
        };
        self.delay_ms = pos_ms - self.cues[i].start_ms;
        Some(self.delay_ms)
    }

    /// The cue the list panel highlights (SUB-015): the last one that started;
    /// in a gap it stays on the previous cue.
    pub fn list_position(&self, pos_ms: i64) -> Option<usize> {
        self.floor(self.cue_time(pos_ms))
    }

    /// The cues whose text contains `query` (case-insensitive), in order.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.to_lowercase();
        self.cues
            .iter()
            .enumerate()
            .filter(|(_, c)| c.plain_text.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    /// In a filtered list (`shown`, ascending cue indices), the row to
    /// highlight: the last shown cue that started (SUB-015).
    pub fn list_row(&self, shown: &[usize], pos_ms: i64) -> Option<usize> {
        let current = self.list_position(pos_ms)?;
        shown.partition_point(|&i| i <= current).checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::Markup;

    fn timeline(spans: &[(i64, i64)]) -> Timeline {
        Timeline::new(
            spans
                .iter()
                .enumerate()
                .map(|(order, &(start_ms, end_ms))| Cue {
                    start_ms,
                    end_ms,
                    plain_text: format!("cue {order}"),
                    markup: Markup::default(),
                    style: None,
                    layer: 0,
                    order,
                })
                .collect(),
        )
    }

    // T-SUB-011
    #[test]
    fn the_offset_moves_activity_and_jumps() {
        let mut t = timeline(&[(1000, 2000)]);
        t.delay_ms = 500;
        assert_eq!(t.active(1600), [0]);
        assert!(t.active(1400).is_empty());
        assert_eq!(t.next(0, 3), Jump::To(1000 + 500 - 180));
        assert_eq!(t.nudge(-1), 400);
    }

    // T-SUB-012
    #[test]
    fn navigation_lands_before_the_cue_but_not_before_the_previous() {
        let t = timeline(&[(1000, 1100), (1150, 2000), (5000, 6000)]);
        assert_eq!(t.previous(5500, false, 3), Jump::To(1000));
        assert_eq!(t.next(1200, 3), Jump::To(4820));
        // in the gap after the second cue, "previous" replays it
        assert_eq!(t.previous(3000, false, 3), Jump::To(1000));
        assert_eq!(t.next(5500, 3), Jump::Seek(3));
        assert_eq!(timeline(&[]).previous(5000, false, 3), Jump::Seek(-3));
        // from the keyboard, a previous cue more than 10 s away is a time seek
        let far = timeline(&[(1000, 2000), (30_000, 31_000)]);
        assert_eq!(far.previous(30_500, true, 3), Jump::Seek(-3));
        assert_eq!(far.previous(30_500, false, 3), Jump::To(820));
    }

    #[test]
    fn overlapping_and_long_cues_stay_active() {
        let t = timeline(&[(0, 10_000), (1000, 2000), (3000, 4000)]);
        assert_eq!(t.active(3500), [0, 2]);
        assert_eq!(t.replay(3500), Some((3000, 4000)));
        assert_eq!(t.replay(9000), Some((0, 10_000)));
    }

    // T-SUB-015
    #[test]
    fn the_list_follows_playback_and_stays_in_gaps() {
        let t = timeline(&[(1000, 2000), (3000, 4000), (5000, 6000)]);
        assert_eq!(t.list_position(500), None);
        assert_eq!(t.list_position(2500), Some(0));
        assert_eq!(t.list_position(5500), Some(2));
        // a search keeps the position among the cues it shows
        let shown = [0, 2];
        assert_eq!(t.list_row(&shown, 3500), Some(0));
        assert_eq!(t.list_row(&shown, 5100), Some(1));
        assert_eq!(t.search("CUE 1"), [1]);
    }

    // T-SUB-016
    #[test]
    fn aligning_the_next_cue_to_the_playhead() {
        let mut t = timeline(&[(1000, 2000), (5000, 6000)]);
        assert_eq!(t.align(4900, true), Some(-100));
        assert_eq!(t.active(4900), [1]);
        assert_eq!(t.align(1500, false), Some(500));
    }
}
