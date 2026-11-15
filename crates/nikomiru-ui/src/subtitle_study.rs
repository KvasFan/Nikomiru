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

/// Optional inclusive full cue duration bounds in milliseconds (SUB-044).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DurationBounds {
    /// Missing means no minimum.
    pub min: Option<u64>,
    /// Missing means no maximum.
    pub max: Option<u64>,
}
impl DurationBounds {
    /// Use wide subtraction to reject malformed times without overflow.
    pub fn keeps(self, cue: &nikomiru_subs::Cue) -> bool {
        if self == Self::default() {
            return true;
        }
        let duration = i128::from(cue.end_ms) - i128::from(cue.start_ms);
        duration > 0
            && self.min.is_none_or(|min| duration >= i128::from(min))
            && self.max.is_none_or(|max| duration <= i128::from(max))
    }
}

/// Inclusive non-whitespace Unicode scalar bounds for subtitle text (SUB-049).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextLength {
    /// Optional inclusive minimum.
    pub min: Option<u32>,
    /// Optional inclusive maximum.
    pub max: Option<u32>,
}
impl TextLength {
    /// Original text is counted without rewriting spacing or markup.
    pub fn keeps(self, text: &str) -> bool {
        let n = text.chars().filter(|c| !c.is_whitespace()).count();
        self.min.is_none_or(|min| n >= min as usize) && self.max.is_none_or(|max| n <= max as usize)
    }
}

/// Caption structure filter; whitespace-only lines do not count (SUB-051).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineShape {
    /// Keep every caption, including empty text.
    #[default]
    Any,
    /// Exactly one nonempty text line.
    Single,
    /// At least two nonempty text lines.
    Multiple,
}
impl LineShape {
    /// Count only as far as the second nonempty line.
    pub fn keeps(self, text: &str) -> bool {
        let n = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .count();
        match self {
            Self::Any => true,
            Self::Single => n == 1,
            Self::Multiple => n == 2,
        }
    }
}

/// Metrics of a bounded subtitle study scope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Statistics {
    /// Unique original cue indices in the scope.
    pub cues: usize,
    /// Non-whitespace Unicode scalar values.
    pub characters: u64,
    /// Union of positive effective dialogue intervals, clipped at time zero.
    pub dialogue_ms: u64,
}

/// Count scope metrics without allocating or exporting sentence snapshots.
pub fn statistics(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    delay_ms: i64,
) -> Result<Statistics, &'static str> {
    if indices.len() > 100_000 {
        return Err("sub-export-too-large");
    }
    let indices: std::collections::BTreeSet<_> = indices.iter().copied().collect();
    let mut result = Statistics {
        cues: indices.len(),
        ..Default::default()
    };
    let mut bytes = 0usize;
    let mut intervals = Vec::new();
    for i in indices {
        let cue = cues.get(i).ok_or("sub-export-unavailable")?;
        bytes = bytes.saturating_add(cue.plain_text.len());
        if bytes > 16 << 20 {
            return Err("sub-export-too-large");
        }
        result.characters = result.characters.saturating_add(
            cue.plain_text
                .chars()
                .filter(|c| !c.is_whitespace())
                .count() as u64,
        );
        let start = cue.start_ms.saturating_add(delay_ms).max(0);
        let end = cue.end_ms.saturating_add(delay_ms);
        if end > start {
            intervals.push((start, end));
        }
    }
    intervals.sort_unstable();
    let mut merged: Option<(i64, i64)> = None;
    for (start, end) in intervals {
        match merged {
            Some((from, until)) if start <= until => merged = Some((from, until.max(end))),
            Some((from, until)) => {
                result.dialogue_ms = result.dialogue_ms.saturating_add((until - from) as u64);
                merged = Some((start, end));
            }
            None => merged = Some((start, end)),
        }
    }
    if let Some((from, until)) = merged {
        result.dialogue_ms = result.dialogue_ms.saturating_add((until - from) as u64);
    }
    Ok(result)
}

/// A bounded index of effective starts for explicit matching-line navigation.
#[derive(Debug, Clone)]
pub struct Navigation {
    entries: Vec<(i64, usize)>,
}

impl Navigation {
    /// Capture a valid scope once; subsequent neighbour queries use binary search.
    pub fn new(cues: &[nikomiru_subs::Cue], indices: &[usize], delay_ms: i64) -> Option<Self> {
        if indices.len() > 100_000 {
            return None;
        }
        let mut entries = Vec::with_capacity(indices.len());
        for &i in indices {
            entries.push((cues.get(i)?.start_ms.saturating_add(delay_ms).max(0), i));
        }
        entries.sort_unstable();
        entries.dedup();
        Some(Self { entries })
    }

    /// Nearest strict previous/next start, excluding a 20 ms equality margin; never wrap.
    pub fn neighbour(&self, position_ms: i64, forward: bool) -> Option<usize> {
        let at = if forward {
            self.entries
                .partition_point(|(start, _)| *start <= position_ms.saturating_add(20))
        } else {
            self.entries
                .partition_point(|(start, _)| *start < position_ms.saturating_sub(20))
                .checked_sub(1)?
        };
        self.entries.get(at).map(|row| row.1)
    }
}

/// Finite cue repetition; one boundary is consumed only after observing that pass.
#[derive(Debug, Clone)]
pub struct Repetition {
    /// Effective start clipped at time zero.
    pub start_ms: i64,
    /// Effective exclusive end.
    pub end_ms: i64,
    /// Passes remaining, including the current pass; zero means finished and paused.
    pub left: u8,
    armed: bool,
}
impl Repetition {
    /// Only the explicit finite choices are accepted.
    pub fn new(cue: &nikomiru_subs::Cue, delay: i64, passes: u8) -> Option<Self> {
        let start_ms = cue.start_ms.saturating_add(delay).max(0);
        let end_ms = cue.end_ms.saturating_add(delay);
        (matches!(passes, 2 | 3 | 5) && end_ms > start_ms).then_some(Self {
            start_ms,
            end_ms,
            left: passes,
            armed: false,
        })
    }
    /// Arm after actual playback returns inside the captured cue, ignoring late end events.
    pub fn position(&mut self, position_ms: i64) {
        if self.left > 0 && (self.start_ms..self.end_ms).contains(&position_ms) {
            self.armed = true;
        }
    }
    /// Consume a real end pause once. Some(true) seeks again; Some(false) stays paused.
    pub fn stopped(&mut self, position_ms: i64) -> Option<bool> {
        if !self.armed || self.left == 0 || position_ms < self.end_ms.saturating_sub(100) {
            return None;
        }
        self.armed = false;
        self.left -= 1;
        Some(self.left > 0)
    }
}

/// Freeze a complete bounded finite-practice queue before issuing any player command (SUB-055).
pub fn repetition_queue(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    delay: i64,
    passes: u8,
    duration_ms: Option<i64>,
) -> Result<std::collections::VecDeque<Repetition>, &'static str> {
    if indices.is_empty() || indices.len() > 1000 {
        return Err("sub-practice-unavailable");
    }
    let mut queue = std::collections::VecDeque::new();
    for i in indices
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let cue = cues.get(i).ok_or("sub-practice-unavailable")?;
        let repetition = Repetition::new(cue, delay, passes).ok_or("sub-practice-unavailable")?;
        if duration_ms.is_some_and(|end| repetition.end_ms > end) {
            return Err("sub-practice-unavailable");
        }
        queue.push_back(repetition);
    }
    Ok(queue)
}

/// Expand exact original indices by a small bounded context radius (SUB-053).
pub fn context_indices(cue_count: usize, exact: &[usize], radius: u8) -> Option<Vec<usize>> {
    if cue_count > 100_000 || exact.len() > 100_000 || radius > 2 {
        return None;
    }
    let mut indices = std::collections::BTreeSet::new();
    for &i in exact {
        if i >= cue_count {
            return None;
        }
        indices.extend(
            i.saturating_sub(usize::from(radius))
                ..=i.saturating_add(usize::from(radius)).min(cue_count - 1),
        );
    }
    Some(indices.into_iter().collect())
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
    // T-SUB-055: queues are ordered/deduplicated and invalid requests never return a partial plan.
    #[test]
    fn finite_practice_queue_validates_every_captured_interval() {
        let cues = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:04,000\n犬\n",
        )
        .cues;
        let q = repetition_queue(&cues, &[1, 0, 1], -500, 2, Some(4000)).unwrap();
        assert_eq!(
            q.iter()
                .map(|r| (r.start_ms, r.end_ms, r.left))
                .collect::<Vec<_>>(),
            [(500, 1500, 2), (2500, 3500, 2)]
        );
        assert!(repetition_queue(&cues, &[0, 9], 0, 2, None).is_err());
        assert!(repetition_queue(&cues, &[0, 1], 0, 2, Some(3000)).is_err());
        assert!(repetition_queue(&cues, &[], 0, 2, None).is_err());
        assert!(repetition_queue(&cues, &vec![0; 1001], 0, 2, None).is_err());
        assert!(repetition_queue(&cues, &[0], 0, 4, None).is_err());
    }

    // T-SUB-053: bounded overlapping context keeps original order and rejects stale indices.
    #[test]
    fn concordance_context_merges_overlaps_and_clips_edges() {
        assert_eq!(context_indices(5, &[0, 2, 2], 1), Some(vec![0, 1, 2, 3]));
        assert_eq!(context_indices(5, &[4], 2), Some(vec![2, 3, 4]));
        assert_eq!(context_indices(5, &[2], 0), Some(vec![2]));
        assert_eq!(context_indices(0, &[], 1), Some(vec![]));
        assert_eq!(context_indices(5, &[5], 1), None);
        assert_eq!(context_indices(5, &[1], 3), None);
    }

    // T-SUB-051: single/multiline structure ignores blank CRLF/Unicode whitespace lines.
    #[test]
    fn caption_structure_counts_nonempty_lines_only() {
        assert!(LineShape::Any.keeps(""));
        assert!(!LineShape::Single.keeps("\n　"));
        assert!(LineShape::Single.keeps("\n猫\r\n\t"));
        assert!(LineShape::Multiple.keeps("猫\r\n\n犬"));
        assert!(!LineShape::Multiple.keeps("猫 犬"));
    }

    // T-SUB-049: scalar rather than byte/grapheme bounds, whitespace and reversed ranges.
    #[test]
    fn text_length_bounds_are_inclusive_and_unicode_aware() {
        let b = TextLength {
            min: Some(2),
            max: Some(2),
        };
        assert!(b.keeps("猫 🐈\n"));
        assert!(!b.keeps("猫"));
        assert!(!b.keeps("猫🐈鳥"));
        assert!(b.keeps("e\u{301}"));
        assert!(TextLength::default().keeps("\t"));
        assert!(
            !TextLength {
                min: Some(3),
                max: Some(2)
            }
            .keeps("猫🐈")
        );
    }

    // T-SUB-048: finite passes ignore duplicate boundaries and cannot wrap after completion.
    #[test]
    fn repetition_consumes_each_observed_pass_once() {
        let mut cue = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫\n",
        )
        .cues
        .remove(0);
        assert!(Repetition::new(&cue, 0, 0).is_none());
        assert!(Repetition::new(&cue, 0, 255).is_none());
        for passes in [2, 3, 5] {
            let mut r = Repetition::new(&cue, -1500, passes).unwrap();
            assert_eq!((r.start_ms, r.end_ms), (0, 500));
            assert_eq!(r.stopped(500), None);
            for n in (0..passes).rev() {
                r.position(0);
                assert_eq!(r.stopped(200), None);
                assert_eq!(r.stopped(500), Some(n > 0));
                assert_eq!(r.left, n);
                assert_eq!(r.stopped(500), None);
            }
            r.position(0);
            assert_eq!(r.stopped(500), None);
        }
        cue.end_ms = cue.start_ms;
        assert!(Repetition::new(&cue, 0, 2).is_none());
    }

    // T-SUB-044: millisecond boundaries and extreme/invalid intervals are exact.
    #[test]
    fn duration_bounds_are_inclusive_and_do_not_overflow() {
        let mut cue = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫\n",
        )
        .cues
        .remove(0);
        let bounds = DurationBounds {
            min: Some(1000),
            max: Some(1000),
        };
        assert!(bounds.keeps(&cue));
        cue.end_ms += 1;
        assert!(!bounds.keeps(&cue));
        cue.end_ms = cue.start_ms;
        assert!(!bounds.keeps(&cue));
        assert!(DurationBounds::default().keeps(&cue));
        cue.start_ms = i64::MIN;
        cue.end_ms = i64::MAX;
        assert!(
            DurationBounds {
                min: Some(u64::MAX),
                max: Some(u64::MAX)
            }
            .keeps(&cue)
        );
        assert!(
            !DurationBounds {
                min: Some(2),
                max: Some(1)
            }
            .keeps(&cue)
        );
    }

    // T-SUB-043: nearest strict starts respect the filtered scope, offsets, equality margin and unavailable edges.
    #[test]
    fn navigation_uses_original_cues_without_wrapping_or_stale_indices() {
        let cues = nikomiru_subs::parse(nikomiru_subs::Format::Srt, "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:04,000\n犬\n\n3\n00:00:05,000 --> 00:00:06,000\n鳥\n").cues;
        let nav = Navigation::new(&cues, &[2, 0, 2], 0).unwrap();
        assert_eq!(nav.neighbour(3000, false), Some(0));
        assert_eq!(nav.neighbour(3000, true), Some(2));
        assert_eq!(nav.neighbour(5001, true), None);
        assert_eq!(nav.neighbour(5001, false), Some(0));
        assert_eq!(nav.neighbour(1000, false), None);
        assert_eq!(
            Navigation::new(&cues, &[0], 100)
                .unwrap()
                .neighbour(1000, true),
            Some(0)
        );
        assert!(Navigation::new(&cues, &[0, 9], 0).is_none());
        assert!(nav.neighbour(i64::MAX, true).is_none());
    }

    // T-SUB-041: union duration avoids double-counting overlaps; scope identities and Unicode counts stay exact.
    #[test]
    fn study_statistics_merge_intervals_and_reject_partial_metrics() {
        let cues = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:03,000\n猫 🐈\n\n2\n00:00:02,000 --> 00:00:04,000\n犬\n",
        )
        .cues;
        assert_eq!(
            statistics(&cues, &[1, 0, 1], 0).unwrap(),
            Statistics {
                cues: 2,
                characters: 3,
                dialogue_ms: 3000
            }
        );
        assert_eq!(statistics(&cues, &[0, 1], -2500).unwrap().dialogue_ms, 1500);
        assert_eq!(statistics(&cues, &[], 0).unwrap(), Statistics::default());
        assert!(statistics(&cues, &[0, 9], 0).is_err());
        assert_eq!(statistics(&cues, &[0], i64::MAX).unwrap().dialogue_ms, 0);
    }

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
