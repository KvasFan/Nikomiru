// SPDX-License-Identifier: GPL-3.0-only

//! Portable current-timeline snapshots (SUB-034).

/// Plain SRT with the captured delay; no output for a partial/empty timeline.
pub fn srt(
    cues: &[nikomiru_subs::Cue],
    delay_ms: i64,
    complete: bool,
) -> Result<String, &'static str> {
    if !complete || cues.is_empty() {
        return Err("sub-export-unavailable");
    }
    if cues.len() > 100_000
        || cues
            .iter()
            .try_fold(0usize, |n, c| n.checked_add(c.plain_text.len()))
            .is_none_or(|bytes| bytes > 16 << 20)
    {
        return Err("sub-export-too-large");
    }
    let rows: Vec<_> = cues
        .iter()
        .filter_map(|cue| {
            let start = cue.start_ms.saturating_add(delay_ms).max(0);
            let end = cue.end_ms.saturating_add(delay_ms);
            (end > start && !cue.plain_text.trim().is_empty())
                .then(|| (start, end, cue.plain_text.clone()))
        })
        .collect();
    if rows.is_empty() {
        return Err("sub-export-unavailable");
    }
    let text = nikomiru_subs::srt::write(&rows);
    if text.len() > 16 << 20 {
        return Err("sub-export-too-large");
    }
    Ok(text)
}

/// Selected original cue indices, deduplicated and emitted in timeline order.
pub fn selected_srt(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    delay_ms: i64,
    complete: bool,
) -> Result<String, &'static str> {
    let selected = selected(cues, indices, complete)?;
    srt(&selected, delay_ms, true)
}

/// A bounded selection snapshot; an invalid index rejects the whole request.
pub fn selected(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    complete: bool,
) -> Result<Vec<nikomiru_subs::Cue>, &'static str> {
    if !complete || indices.is_empty() {
        return Err("sub-export-unavailable");
    }
    if indices.len() > 100_000 {
        return Err("sub-export-too-large");
    }
    let indices: std::collections::BTreeSet<_> = indices.iter().copied().collect();
    let mut bytes = 0usize;
    indices
        .into_iter()
        .map(|i| {
            let cue = cues.get(i).ok_or("sub-export-unavailable")?;
            bytes = bytes.saturating_add(cue.plain_text.len());
            if bytes > 16 << 20 {
                return Err("sub-export-too-large");
            }
            Ok(cue.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-SUB-035: original indices retain chronology; stale/invalid/partial selections fail closed.
    #[test]
    fn selected_exports_keep_original_cues_and_reject_invalid_indices() {
        let cues = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:04,000\n犬\n",
        )
        .cues;
        let text = selected_srt(&cues, &[1, 0, 1], 500, true).unwrap();
        let parsed = nikomiru_subs::parse(nikomiru_subs::Format::Srt, &text);
        assert_eq!(parsed.cues.len(), 2);
        assert_eq!(parsed.cues[0].plain_text, "猫");
        assert_eq!(parsed.cues[1].start_ms, 3500);
        assert_eq!(
            selected_srt(&cues, &[0, 9], 0, true),
            Err("sub-export-unavailable")
        );
        assert_eq!(
            selected_srt(&cues, &[0], 0, false),
            Err("sub-export-unavailable")
        );
        assert_eq!(
            selected_srt(&cues, &[], 0, true),
            Err("sub-export-unavailable")
        );
    }

    // T-SUB-034: plain complete snapshots preserve overlaps/lines and apply captured delay.
    #[test]
    fn export_applies_delay_clips_negative_intervals_and_rejects_partial_tracks() {
        let cues = nikomiru_subs::parse(nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n食べる\n日本語\n\n2\n00:00:01,500 --> 00:00:04,000\n猫\n").cues;
        let text = srt(&cues, 250, true).unwrap();
        let parsed = nikomiru_subs::parse(nikomiru_subs::Format::Srt, &text);
        assert_eq!(
            (parsed.cues[0].start_ms, parsed.cues[0].end_ms),
            (1250, 2250)
        );
        assert_eq!(parsed.cues[0].plain_text, "食べる\n日本語");
        assert_eq!(parsed.cues[1].start_ms, 1750);
        let clipped = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            &srt(&cues, -2500, true).unwrap(),
        );
        assert_eq!(clipped.cues.len(), 1);
        assert_eq!(
            (clipped.cues[0].start_ms, clipped.cues[0].end_ms),
            (0, 1500)
        );
        assert_eq!(srt(&cues, 0, false), Err("sub-export-unavailable"));
        assert_eq!(srt(&cues, i64::MAX, true), Err("sub-export-unavailable"));
        assert_eq!(srt(&cues, -5000, true), Err("sub-export-unavailable"));
        let mut large = cues;
        large[0].plain_text = "a".repeat((16 << 20) + 1);
        assert_eq!(srt(&large, 0, true), Err("sub-export-too-large"));
    }
}
