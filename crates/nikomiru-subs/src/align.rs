// SPDX-License-Identifier: GPL-3.0-only

//! Automatic alignment with the audio (SUB-018): the subtitle offset that
//! makes the cues fall on the moments the audio is loud. The cues and the
//! audio levels become activity series on one grid of [`BIN_MS`]; the offset
//! is the shift within ±[`MAX_SHIFT_MS`] where the two correlate best.
//!
//! Translated from Fushi `fushi/lib/src/media/video/subtitle_auto_align.dart`
//! (commit `ac8034e`), with one change: the score is the Pearson correlation
//! of the two series, not their overlap over the smaller one, which is high
//! at any shift when the audio is loud most of the time (Fushi's TODO-413).

/// The grid, in milliseconds.
pub const BIN_MS: i64 = 100;

/// The largest offset looked for.
pub const MAX_SHIFT_MS: i64 = 15_000;

/// The correlation below which an offset is not applied.
pub const MIN_CONFIDENCE: f64 = 0.15;

/// How far the best correlation must stand above the best one more than
/// [`NEAR_MS`] away: two offsets that fit about as well are no answer.
pub const MIN_MARGIN: f64 = 0.05;

/// The shifts around the best one that belong to its peak.
pub const NEAR_MS: i64 = 1_000;

/// The audio analysed: the first 20 minutes find a constant offset.
pub const ANALYSIS_MS: i64 = 20 * 60_000;

/// A level, relative to the quietest and the loudest of the track, from which
/// a bin counts as sound.
const VOICE: f32 = 0.35;

/// An offset and how sure it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    /// The subtitle offset, like `delay_ms`: positive moves the cues later.
    pub offset_ms: i64,
    /// The correlation at that offset, −1…1.
    pub confidence: f64,
    /// How far it stands above the best correlation away from it.
    pub margin: f64,
}

/// What an alignment found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignResult {
    /// An offset to apply.
    Aligned(Alignment),
    /// The best offset is not sure enough to apply.
    Unsure(Alignment),
    /// No cues, or no sound.
    NoData,
}

/// The cues `[start, end)` (milliseconds, without the offset) as an activity
/// series over `[0, duration_ms)`: 1 where a cue is, 0 elsewhere. A cue past
/// the end is left out.
pub fn cue_activity(cues: &[(i64, i64)], duration_ms: i64) -> Vec<f32> {
    if duration_ms <= 0 {
        return Vec::new();
    }
    let len = usize::try_from((duration_ms + BIN_MS - 1) / BIN_MS).unwrap_or(0);
    let mut out = vec![0.0; len];
    for &(start, end) in cues {
        if end <= start || start >= duration_ms || end <= 0 {
            continue;
        }
        let first = usize::try_from(start.max(0) / BIN_MS).unwrap_or(0);
        let last = usize::try_from((end.min(duration_ms) - 1) / BIN_MS).unwrap_or(0);
        for bin in out.iter_mut().take(last + 1).skip(first) {
            *bin = 1.0;
        }
    }
    out
}

/// Audio levels (dB, one per bin) as an activity series: 1 where the level is
/// in the loud part of the track's own range, 0 elsewhere. Silence (`-inf`,
/// NaN) is 0; a track of one level is all 0.
pub fn sound_activity(levels_db: &[f32]) -> Vec<f32> {
    let finite = levels_db.iter().copied().filter(|v| v.is_finite());
    let (min, max) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if max.partial_cmp(&min) != Some(std::cmp::Ordering::Greater) {
        return vec![0.0; levels_db.len()];
    }
    levels_db
        .iter()
        .map(|&v| {
            if v.is_finite() && (v - min) / (max - min) >= VOICE {
                1.0
            } else {
                0.0
            }
        })
        .collect()
}

/// The Pearson correlation of `audio[i]` with `cues[i - shift]` where both
/// exist; `None` without variation, or when they overlap on less than half
/// of the shorter series (a few bins agree by chance).
fn correlation(audio: &[f32], cues: &[f32], shift: i64) -> Option<f64> {
    let n = audio.len() as i64;
    let m = cues.len() as i64;
    let from = shift.max(0);
    let to = n.min(m + shift);
    if to - from < 2 || 2 * (to - from) < n.min(m) {
        return None;
    }
    let pairs = || {
        (from..to).map(move |i| {
            let a = audio[usize::try_from(i).unwrap_or(0)];
            let c = cues[usize::try_from(i - shift).unwrap_or(0)];
            (f64::from(a), f64::from(c))
        })
    };
    let count = (to - from) as f64;
    let (sa, sc) = pairs().fold((0.0, 0.0), |(x, y), (a, c)| (x + a, y + c));
    let (ma, mc) = (sa / count, sc / count);
    let (mut cov, mut va, mut vc) = (0.0, 0.0, 0.0);
    for (a, c) in pairs() {
        cov += (a - ma) * (c - mc);
        va += (a - ma) * (a - ma);
        vc += (c - mc) * (c - mc);
    }
    (va > 0.0 && vc > 0.0).then(|| cov / (va * vc).sqrt())
}

/// The offset within ±[`MAX_SHIFT_MS`] where `audio` and `cues` (activity
/// series on the same grid) correlate best; on a tie the smaller offset.
pub fn best_offset(audio: &[f32], cues: &[f32]) -> AlignResult {
    if !audio.iter().any(|&a| a > 0.0) || !cues.iter().any(|&c| c > 0.0) {
        return AlignResult::NoData;
    }
    let max_shift = MAX_SHIFT_MS / BIN_MS;
    let scores: Vec<(i64, f64)> = (-max_shift..=max_shift)
        .filter_map(|shift| correlation(audio, cues, shift).map(|r| (shift, r)))
        .collect();
    let mut best: Option<(i64, f64)> = None;
    for &(shift, r) in &scores {
        let better = match best {
            None => true,
            Some((s, b)) => r > b || (r == b && shift.abs() < s.abs()),
        };
        if better {
            best = Some((shift, r));
        }
    }
    let Some((shift, r)) = best else {
        return AlignResult::NoData;
    };
    let second = scores
        .iter()
        .filter(|(s, _)| (s - shift).abs() * BIN_MS > NEAR_MS)
        .map(|(_, r)| *r)
        .fold(f64::NEG_INFINITY, f64::max);
    let margin = if second.is_finite() { r - second } else { r };
    let found = Alignment {
        offset_ms: shift * BIN_MS,
        confidence: r,
        margin,
    };
    if r >= MIN_CONFIDENCE && margin >= MIN_MARGIN {
        AlignResult::Aligned(found)
    } else {
        AlignResult::Unsure(found)
    }
}

/// The offset of `cues` (without their offset) against the audio levels of
/// the first `levels_db.len()` bins.
pub fn align(cues: &[(i64, i64)], levels_db: &[f32]) -> AlignResult {
    let duration = levels_db.len() as i64 * BIN_MS;
    best_offset(&sound_activity(levels_db), &cue_activity(cues, duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speech-like audio: loud in `windows` (ms), quiet elsewhere, a little
    /// noise everywhere.
    fn levels(windows: &[(i64, i64)], duration_ms: i64) -> Vec<f32> {
        (0..duration_ms / BIN_MS)
            .map(|b| {
                let t = b * BIN_MS;
                let noise = ((b * 7919) % 13) as f32 * 0.3;
                if windows.iter().any(|&(s, e)| t >= s && t < e) {
                    -20.0 + noise
                } else {
                    -60.0 + noise
                }
            })
            .collect()
    }

    /// Lines of irregular length at irregular moments, as in speech.
    fn lines(count: i64, gap: i64) -> Vec<(i64, i64)> {
        let mut seed: u64 = 7;
        let mut random = move |n: i64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) as i64 % n
        };
        let mut t = 3_000;
        (0..count)
            .map(|_| {
                let start = t + random(gap);
                let end = start + 600 + random(2_000);
                t = end + 200;
                (start, end)
            })
            .collect()
    }

    // T-SUB-018: Fushi `subtitle_auto_align_test.dart`, `buildCueActivityEnvelope`
    #[test]
    fn cues_on_the_grid() {
        assert_eq!(
            cue_activity(&[(100, 300), (450, 460), (900, 2000), (5, 5)], 1000),
            [0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
        );
        assert!(cue_activity(&[(0, 100)], 0).is_empty());
        // past the end: left out, not piled on the last bin
        assert_eq!(cue_activity(&[(5000, 6000)], 300), [0.0, 0.0, 0.0]);
        assert_eq!(
            sound_activity(&[-60.0, f32::NEG_INFINITY, -20.0, -50.0, f32::NAN]),
            [0.0, 0.0, 1.0, 0.0, 0.0]
        );
        assert_eq!(sound_activity(&[-30.0, -30.0]), [0.0, 0.0]);
    }

    // T-SUB-018
    #[test]
    fn the_offset_is_found_both_ways() {
        // the first 20 minutes, as analysed
        let speech = lines(400, 2_500);
        let audio = levels(&speech, ANALYSIS_MS);
        for offset in [0, 1_200, -3_400, 9_900, -14_800] {
            // cues written `offset` too early: they need `+offset`
            let cues: Vec<(i64, i64)> = speech
                .iter()
                .map(|&(s, e)| (s - offset, e - offset))
                .collect();
            match align(&cues, &audio) {
                AlignResult::Aligned(a) => {
                    // a line starting inside a bin: one bin either way
                    assert!((a.offset_ms - offset).abs() <= BIN_MS, "{offset}: {a:?}");
                    assert!(a.confidence > 0.8, "{a:?}");
                }
                other => panic!("{offset}: {other:?}"),
            }
        }
    }

    // T-SUB-018: Fushi's TODO-413, loud audio everywhere
    #[test]
    fn unrelated_audio_is_not_aligned() {
        let speech = lines(400, 2_500);
        // other sounds at moments that have nothing to do with the cues
        let mut seed: u64 = 42;
        let mut random = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) as i64
        };
        let others: Vec<(i64, i64)> = (0..700)
            .map(|_| {
                let start = random() % ANALYSIS_MS;
                (start, start + 300 + random() % 2_500)
            })
            .collect();
        let audio = levels(&others, ANALYSIS_MS);
        assert!(
            !matches!(align(&speech, &audio), AlignResult::Aligned(_)),
            "{:?}",
            align(&speech, &audio)
        );
        assert_eq!(align(&[], &audio), AlignResult::NoData);
        assert_eq!(align(&speech, &vec![-40.0; 1000]), AlignResult::NoData);
    }
}
