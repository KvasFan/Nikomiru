// SPDX-License-Identifier: GPL-3.0-only

//! Temporary speed overrides for the gaps between subtitle lines (PLY-022).

use nikomiru_subs::timeline::Timeline;

/// Leave dialogue and its edges at the user's speed. The lead also covers
/// several 30 Hz position events at the maximum speed of 4×.
const EDGE_MS: i64 = 500;
const MIN_GAP_MS: i64 = 3_000;

pub(crate) fn in_gap(timeline: &Timeline, pos_ms: i64, normal: f64, faster: f64) -> bool {
    // A rate change must drain audio already queued at the previous rate.
    // Reserve half a second of wall time for that queue, in addition to
    // the dialogue lead and the position-event/command margin. A fixed
    // 700 ms lead lets 4× -> 1× reach the next line with mpv's null output.
    let queued = ((faster - normal).max(0.0) * 500.0).ceil() as i64;
    let lead = EDGE_MS + 200 + queued;
    timeline.gap(pos_ms).is_some_and(|(end, start)| {
        start.saturating_sub(end) >= MIN_GAP_MS
            && pos_ms >= end.saturating_add(EDGE_MS)
            && pos_ms < start.saturating_sub(lead)
    })
}

/// Keep the original speed until mpv acknowledges its restoration. A queued
/// speed event from the override must never become the next original speed.
#[derive(Debug, Default)]
pub(crate) struct GapSpeed {
    normal: Option<f64>,
    requested: Option<f64>,
}

impl GapSpeed {
    pub(crate) fn normal(&self, actual: f64) -> f64 {
        self.normal.unwrap_or(actual)
    }

    pub(crate) fn observed(&mut self, actual: f64) {
        if self.normal == Some(actual) && self.requested == Some(actual) {
            self.normal = None;
            self.requested = None;
        }
    }

    /// A user command or a file's saved speed wins over pending events.
    pub(crate) fn chosen(&mut self, speed: f64) {
        self.normal = Some(speed);
        self.requested = Some(speed);
    }

    /// The next command, only when the desired speed changes. `None` for the
    /// override restores the normal speed (pause, replay, settings, etc.).
    pub(crate) fn update(&mut self, actual: f64, override_speed: Option<f64>) -> Option<f64> {
        let normal = self.normal(actual);
        let target = override_speed.filter(|s| *s > normal).unwrap_or(normal);
        if self.normal.is_none() {
            if target == actual {
                return None;
            }
            self.normal = Some(actual);
        }
        if self.requested == Some(target) {
            return None;
        }
        self.requested = Some(target);
        Some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-PLY-022
    #[test]
    fn only_long_gaps_accelerate_with_room_for_dialogue_at_both_edges() {
        let track = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\na\n\n\
             2\n00:00:05,000 --> 00:00:06,000\nb\n\n\
             3\n00:00:08,000 --> 00:00:09,000\nc\n",
        );
        let mut timeline = Timeline::new(track.cues);
        for pos in [
            0, 1000, 1999, 2000, 2499, 2800, 3000, 4500, 5000, 7000, 10_000,
        ] {
            assert!(!in_gap(&timeline, pos, 1.0, 4.0), "{pos}");
        }
        for pos in [2500, 2600, 2799] {
            assert!(in_gap(&timeline, pos, 1.0, 4.0), "{pos}");
        }
        assert!(in_gap(&timeline, 3000, 1.0, 2.0));
        timeline.delay_ms = 1000;
        assert!(!in_gap(&timeline, 3000, 1.0, 4.0));
        assert!(in_gap(&timeline, 3500, 1.0, 4.0));
    }

    // T-PLY-022: delayed mpv replies and repeated position events cannot save
    // a temporary speed as the user's choice or send commands every frame.
    #[test]
    fn the_override_restores_the_chosen_speed_despite_late_events() {
        let mut speed = GapSpeed::default();
        assert_eq!(speed.update(1.2, Some(4.0)), Some(4.0));
        assert_eq!(speed.update(1.2, Some(4.0)), None);
        speed.observed(4.0);
        assert_eq!(speed.normal(4.0), 1.2);
        assert_eq!(speed.update(4.0, None), Some(1.2));
        speed.observed(4.0);
        assert_eq!(speed.normal(4.0), 1.2);
        // Seek back into the gap before the restore reply arrives.
        assert_eq!(speed.update(4.0, Some(4.0)), Some(4.0));
        speed.observed(1.2);
        assert_eq!(speed.normal(4.0), 1.2);
        assert_eq!(speed.update(4.0, None), Some(1.2));
        speed.observed(1.2);
        assert_eq!(speed.update(1.2, None), None);
        assert_eq!(speed.normal(1.5), 1.5);
    }

    // T-PLY-022
    #[test]
    fn manual_choices_change_the_restore_speed_and_faster_playback_stays_fast() {
        let mut speed = GapSpeed::default();
        assert_eq!(speed.update(3.0, Some(2.0)), None);
        assert_eq!(speed.update(1.0, Some(4.0)), Some(4.0));
        speed.chosen(1.1);
        speed.observed(4.0);
        assert_eq!(speed.normal(4.0), 1.1);
        assert_eq!(speed.update(4.0, Some(4.0)), Some(4.0));
        assert_eq!(speed.update(4.0, None), Some(1.1));
    }
}
