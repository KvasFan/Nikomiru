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

#[cfg(test)]
mod tests {
    use super::*;
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
