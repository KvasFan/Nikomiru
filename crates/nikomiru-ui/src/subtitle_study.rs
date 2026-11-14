// SPDX-License-Identifier: GPL-3.0-only

//! View-only subtitle study scopes (SUB-039).

/// Inclusive-start/exclusive-end playback-time bounds, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeWindow {
    /// Missing means no lower bound.
    pub from: Option<i64>,
    /// Missing means no upper bound.
    pub to: Option<i64>,
}

impl TimeWindow {
    /// Positive overlap after the track delay; no bounds preserves the original list.
    pub fn keeps(self, cue: &nikomiru_subs::Cue, delay_ms: i64) -> bool {
        if self == Self::default() {
            return true;
        }
        let from = self.from.unwrap_or(0).max(0);
        let to = self.to.unwrap_or(i64::MAX);
        let start = cue.start_ms.saturating_add(delay_ms).max(0);
        let end = cue.end_ms.saturating_add(delay_ms);
        to > from && end > start && start < to && end > from
    }
}

/// Optional inclusive characters-per-second bounds, using non-whitespace Unicode scalar values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadingSpeed {
    /// Missing means no minimum.
    pub min: Option<u32>,
    /// Missing means no maximum.
    pub max: Option<u32>,
}

impl ReadingSpeed {
    /// Compare exact integer ratios using the complete cue duration, independently of delay.
    pub fn keeps(self, cue: &nikomiru_subs::Cue) -> bool {
        if self == Self::default() {
            return true;
        }
        let duration = i128::from(cue.end_ms) - i128::from(cue.start_ms);
        if duration <= 0 {
            return false;
        }
        let duration = duration as u128;
        let characters = cue
            .plain_text
            .chars()
            .filter(|c| !c.is_whitespace())
            .count() as u128;
        let numerator = characters * 1000;
        self.min
            .is_none_or(|min| numerator >= u128::from(min) * duration)
            && self
                .max
                .is_none_or(|max| numerator <= u128::from(max) * duration)
    }
}

/// Exact headword/reading examples captured from one complete primary timeline.
#[derive(Debug, Clone)]
pub struct Concordance {
    /// Original word identity.
    pub key: String,
    /// Original headword and reading for display.
    pub label: String,
    /// Chronological original cue indices.
    pub indices: Vec<usize>,
    /// Primary timeline generation.
    pub version: u64,
    /// Segmentation/status revision captured at the request.
    pub revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-SUB-040: Unicode characters, exact thresholds, invalid ranges and extreme times do not round or overflow.
    #[test]
    fn reading_speed_uses_exact_unicode_counts_and_full_durations() {
        let mut cue = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫 🐈\n",
        )
        .cues
        .remove(0);
        assert!(
            ReadingSpeed {
                min: Some(2),
                max: Some(2)
            }
            .keeps(&cue)
        );
        assert!(
            !ReadingSpeed {
                min: None,
                max: Some(1)
            }
            .keeps(&cue)
        );
        assert!(
            !ReadingSpeed {
                min: Some(3),
                max: Some(2)
            }
            .keeps(&cue)
        );
        cue.end_ms = 2001;
        assert!(
            !ReadingSpeed {
                min: Some(2),
                max: None
            }
            .keeps(&cue)
        );
        cue.start_ms = i64::MIN;
        cue.end_ms = i64::MAX;
        assert!(
            ReadingSpeed {
                min: None,
                max: Some(1)
            }
            .keeps(&cue)
        );
        cue.end_ms = cue.start_ms;
        assert!(
            !ReadingSpeed {
                min: Some(0),
                max: None
            }
            .keeps(&cue)
        );
        assert!(ReadingSpeed::default().keeps(&cue));
    }

    // T-SUB-039: boundary contact is not overlap; delay, negative clips and reversed windows count.
    #[test]
    fn time_windows_use_effective_half_open_intervals() {
        let cue = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫\n",
        )
        .cues
        .remove(0);
        assert!(TimeWindow::default().keeps(&cue, 0));
        let w = TimeWindow {
            from: Some(2000),
            to: Some(3000),
        };
        assert!(!w.keeps(&cue, 0));
        assert!(w.keeps(&cue, 1));
        assert!(
            !TimeWindow {
                from: None,
                to: Some(1000)
            }
            .keeps(&cue, 0)
        );
        assert!(
            !TimeWindow {
                from: Some(3000),
                to: Some(2000)
            }
            .keeps(&cue, 0)
        );
        assert!(
            TimeWindow {
                from: None,
                to: Some(500)
            }
            .keeps(&cue, -1500)
        );
        assert!(
            !TimeWindow {
                from: None,
                to: Some(500)
            }
            .keeps(&cue, i64::MAX)
        );
    }
}
