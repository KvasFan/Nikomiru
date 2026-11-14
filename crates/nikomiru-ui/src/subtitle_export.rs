// SPDX-License-Identifier: GPL-3.0-only

//! Portable current-timeline snapshots (SUB-034).

/// Portable text subtitle snapshot format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// SubRip.
    Srt,
    /// Web Video Text Tracks.
    Vtt,
}

/// Render a selected snapshot with an explicitly chosen format.
pub fn render(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    delay_ms: i64,
    complete: bool,
    format: ExportFormat,
) -> Result<String, &'static str> {
    match format {
        ExportFormat::Srt => selected_srt(cues, indices, delay_ms, complete),
        ExportFormat::Vtt => vtt(cues, indices, delay_ms, complete),
    }
}

/// Escaped plain-text WebVTT, with structural blank lines removed from payloads.
pub fn vtt(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    delay_ms: i64,
    complete: bool,
) -> Result<String, &'static str> {
    let selected = selected(cues, indices, complete)?;
    let mut out = String::from("WEBVTT\n\n");
    let mut written = 0;
    let clock = |ms: i64| {
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        )
    };
    for cue in selected {
        let start = cue.start_ms.saturating_add(delay_ms).max(0);
        let end = cue.end_ms.saturating_add(delay_ms);
        if end <= start || cue.plain_text.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("{} --> {}\n", clock(start), clock(end)));
        for line in cue.plain_text.lines().filter(|l| !l.trim().is_empty()) {
            for c in line.chars() {
                match c {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    _ => out.push(c),
                }
                if out.len() > 16 << 20 {
                    return Err("sub-export-too-large");
                }
            }
            out.push('\n');
        }
        out.push('\n');
        written += 1;
        if out.len() > 16 << 20 {
            return Err("sub-export-too-large");
        }
    }
    if written == 0 {
        return Err("sub-export-unavailable");
    }
    Ok(out)
}

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

/// Copyable transcript of an explicitly captured complete cue selection.
pub fn transcript(
    cues: &[nikomiru_subs::Cue],
    indices: &[usize],
    delay_ms: i64,
    complete: bool,
) -> Result<String, &'static str> {
    let selected = selected(cues, indices, complete)?;
    let mut out = String::from("start_ms\tend_ms\ttext\n");
    let mut written = 0;
    for cue in selected {
        let start = cue.start_ms.saturating_add(delay_ms).max(0);
        let end = cue.end_ms.saturating_add(delay_ms);
        if end <= start || cue.plain_text.trim().is_empty() {
            continue;
        }
        let row = format!(
            "{start}\t{end}\t{}\n",
            crate::word_export::field(&cue.plain_text)
        );
        if out.len().saturating_add(row.len()) > 16 << 20 {
            return Err("sub-export-too-large");
        }
        out.push_str(&row);
        written += 1;
    }
    if written == 0 {
        return Err("sub-export-unavailable");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-SUB-037: VTT round trips escaped Unicode/markup/overlaps without cue injection.
    #[test]
    fn webvtt_snapshots_are_portable_bounded_and_not_partial() {
        let mut cues = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:03,000\n猫\n\n2\n00:00:02,000 --> 00:00:04,000\n犬\n",
        )
        .cues;
        cues[0].plain_text = "<猫> & text\n\n00:00:09.000 --> 00:00:10.000\ninjected".into();
        let text = vtt(&cues, &[1, 0, 1], -1500, true).unwrap();
        let parsed = nikomiru_subs::parse(nikomiru_subs::Format::Vtt, &text);
        assert_eq!(parsed.cues.len(), 2);
        assert_eq!((parsed.cues[0].start_ms, parsed.cues[1].start_ms), (0, 500));
        assert!(
            parsed.cues[0]
                .plain_text
                .starts_with("<猫> & text\n00:00:09.000")
        );
        assert_eq!(vtt(&cues, &[0], 0, false), Err("sub-export-unavailable"));
        cues[0].plain_text = "&".repeat(4 << 20);
        assert_eq!(vtt(&cues, &[0], 0, true), Err("sub-export-too-large"));
    }

    // T-SUB-036: one safe row per selected original cue, with effective times and all-or-nothing bounds.
    #[test]
    fn transcript_snapshot_preserves_timing_and_neutralises_fields() {
        let mut cues = nikomiru_subs::parse(
            nikomiru_subs::Format::Srt,
            "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:04,000\n犬\n",
        )
        .cues;
        cues[1].plain_text = "=formula\nother\ttext".into();
        assert_eq!(
            transcript(&cues, &[1, 0], -1500, true).unwrap(),
            "start_ms\tend_ms\ttext\n0\t500\t猫\n1500\t2500\t'=formula other text\n"
        );
        assert!(transcript(&cues, &[0], -3000, true).is_err());
        assert!(transcript(&cues, &[0], 0, false).is_err());
        cues[0].plain_text = "x".repeat(16 << 20);
        assert_eq!(
            transcript(&cues, &[0], 0, true),
            Err("sub-export-too-large")
        );
    }

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
