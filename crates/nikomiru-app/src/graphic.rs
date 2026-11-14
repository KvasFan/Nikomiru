// SPDX-License-Identifier: GPL-3.0-only

//! Graphic subtitles read as text (SUB-028, SUB-029): the pictures of a
//! PGS or VobSub track, each cut into its lines and read by the manga's model, on part of the
//! processor's cores so the video keeps playing; then neighbours that read
//! the same are one line. As Fushi's whole-track reading
//! (`fushi/lib/src/media/video/graphic_subtitle_track_ocr.dart`, commit
//! `ac8034e`), without its text detector and its AI second reading.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use nikomiru_media::process::Cancel;
use nikomiru_subs::pgs::{Graphic, Picture, lines};

/// Neighbours that read the same, at most this far apart, are one line.
const JOIN_GAP_MS: i64 = 120;

/// A cue read: start and end in milliseconds, and its text.
pub type ReadCue = (i64, i64, String);

/// Reads one line of a picture.
pub type ReadLine<'a> = dyn Fn(&Picture) -> Result<String, String> + Sync + 'a;

/// How many pictures are read at once: half the cores, so the video and
/// the interface keep theirs.
pub fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1))
}

/// The cache file of the text read from stream `ff_index` of `video`, with
/// the model's revision in its name, so another model reads again.
pub fn cache_path(cache_dir: &Path, video: &Path, ff_index: i64) -> std::io::Result<PathBuf> {
    let key = nikomiru_media::extract::video_key(video)?;
    let model = &nikomiru_ocr::MODEL[0].sha256[..8];
    Ok(cache_dir.join(format!("{key}-{ff_index}.read-{model}.srt")))
}

/// The text of each cue, its lines apart by `\n`, read on `threads`
/// threads; `None` when cancelled. A line that cannot be read is left out;
/// the first error is returned only when no line could be read at all.
pub fn read<C: Graphic>(
    cues: &[C],
    threads: usize,
    cancel: &Cancel,
    read_line: &ReadLine<'_>,
) -> Result<Option<Vec<ReadCue>>, String> {
    let next = AtomicUsize::new(0);
    let texts: Mutex<Vec<Option<String>>> = Mutex::new(vec![None; cues.len()]);
    let failure: Mutex<Option<String>> = Mutex::new(None);
    let read_any = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1).min(cues.len().max(1)) {
            scope.spawn(|| {
                loop {
                    if cancel.is_cancelled() {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(cue) = cues.get(i) else { return };
                    let Some(picture) = cue.picture() else {
                        continue;
                    };
                    let mut text = Vec::new();
                    for line in lines(&picture) {
                        let mut read = String::new();
                        for piece in &line {
                            match read_line(piece) {
                                Ok(t) => read.push_str(t.trim()),
                                Err(e) => {
                                    tracing::warn!(
                                        "reading a subtitle at {} ms: {e}",
                                        cue.start_ms()
                                    );
                                    failure
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .get_or_insert(e);
                                }
                            }
                        }
                        if !read.is_empty() {
                            read_any.fetch_add(1, Ordering::Relaxed);
                            text.push(read);
                        }
                    }
                    if !text.is_empty() {
                        texts.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(text.join("\n"));
                    }
                }
            });
        }
    });
    if cancel.is_cancelled() {
        return Ok(None);
    }
    if read_any.load(Ordering::Relaxed) == 0
        && let Some(e) = failure.into_inner().unwrap_or_else(|e| e.into_inner())
    {
        return Err(e);
    }
    let texts = texts.into_inner().unwrap_or_else(|e| e.into_inner());
    let read = cues
        .iter()
        .zip(texts)
        .filter_map(|(cue, text)| Some((cue.start_ms(), cue.end_ms(), text?)));
    Ok(Some(join(read)))
}

/// Neighbours with the same text, touching, as one cue: a picture sent
/// again, or a line whose pictures change only where nothing was read.
fn join(cues: impl Iterator<Item = ReadCue>) -> Vec<ReadCue> {
    let mut out: Vec<ReadCue> = Vec::new();
    for (start, end, text) in cues {
        match out.last_mut() {
            Some(last) if last.2 == text && start - last.1 <= JOIN_GAP_MS => {
                last.1 = last.1.max(end);
            }
            _ => out.push((start, end, text)),
        }
    }
    out
}

/// Pictures per progressive OCR batch (SUB-030).
pub const BATCH: usize = 16;

/// Read current/future pictures before earlier ones, publishing cumulative
/// ordered text after each bounded batch. Cancellation returns no final track;
/// callers must commit their cache only after a complete result.
pub fn read_progressive<C: Graphic>(
    cues: &[C],
    position_ms: i64,
    threads: usize,
    cancel: &Cancel,
    read_line: &ReadLine<'_>,
    progress: &dyn Fn(&[ReadCue], usize, usize),
) -> Result<Option<Vec<ReadCue>>, String> {
    let at = cues
        .iter()
        .position(|c| c.end_ms() > position_ms)
        .unwrap_or(cues.len());
    let mut output = Vec::new();
    let mut done = 0;
    let mut failure = None;
    for region in [&cues[at..], &cues[..at]] {
        for batch in region.chunks(BATCH) {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            match read(batch, threads, cancel, read_line) {
                Ok(Some(read)) => output.extend(read),
                Ok(None) => return Ok(None),
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
            done += batch.len();
            output.sort_by_key(|c| (c.0, c.1));
            output = join(output.into_iter());
            if !output.is_empty() && !cancel.is_cancelled() {
                progress(&output, done, cues.len());
            }
        }
    }
    if cancel.is_cancelled() {
        return Ok(None);
    }
    if output.is_empty()
        && let Some(error) = failure
    {
        return Err(error);
    }
    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_subs::pgs::{PgsCue, parse, testing};

    /// Three lines, two cut apart by a moment with nothing, of widths that
    /// the fake reader reads.
    fn stream() -> Vec<PgsCue> {
        let mut sup = Vec::new();
        testing::display_set(&mut sup, 1000, &[(100, 900, testing::block(60, 20))], 255);
        testing::display_set(&mut sup, 2000, &[(100, 900, testing::block(61, 20))], 255);
        testing::display_set(&mut sup, 2050, &[(100, 900, testing::block(60, 20))], 255);
        testing::display_set(&mut sup, 3000, &[], 255);
        testing::display_set(
            &mut sup,
            4000,
            &[
                (100, 800, testing::block(40, 20)),
                (100, 900, testing::block(80, 20)),
            ],
            255,
        );
        testing::display_set(&mut sup, 5000, &[], 255);
        parse(&sup)
    }

    fn by_width(line: &Picture) -> Result<String, String> {
        Ok(match line.width {
            w if w < 60 => "上".to_string(),
            w if w < 80 => "行くぞ".to_string(),
            _ => "下".to_string(),
        })
    }

    // T-SUB-028: each picture read line by line; neighbours read the same
    // are one line
    #[test]
    fn pictures_are_read_line_by_line() {
        let read = read(&stream(), 3, &Cancel::new(), &by_width)
            .unwrap()
            .unwrap();
        assert_eq!(
            read,
            [
                (1000, 3000, "行くぞ".to_string()),
                (4000, 5000, "上\n下".to_string()),
            ]
        );
    }

    // T-SUB-029: VobSub's pictures are read as PGS's are
    #[test]
    fn dvd_pictures_are_read_too() {
        use nikomiru_subs::vobsub::{self, testing as dvd};
        let (ps, _) = dvd::program_stream(
            &[
                (1000, dvd::spu(0, 0, &testing::block(60, 20), 1000)),
                (3000, dvd::spu(0, 0, &testing::block(40, 20), 1000)),
            ],
            2000,
        );
        let cues = vobsub::parse(&ps, Some(dvd::PALETTE));
        let read = read(&cues, 2, &Cancel::new(), &by_width).unwrap().unwrap();
        let texts: Vec<&str> = read.iter().map(|c| c.2.as_str()).collect();
        assert_eq!(texts, ["行くぞ", "上"]);
        assert_eq!(read[0].0, 1000);
    }

    // T-SUB-028: stopped when cancelled; an error only when nothing reads
    #[test]
    fn a_reading_stops_or_fails_as_a_whole() {
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(read(&stream(), 2, &cancel, &by_width), Ok(None));
        let broken = |_: &Picture| Err::<String, _>("no model".to_string());
        assert_eq!(
            read(&stream(), 2, &Cancel::new(), &broken),
            Err("no model".into())
        );
        let some = |l: &Picture| {
            if l.width < 60 {
                Err("bad".to_string())
            } else {
                by_width(l)
            }
        };
        let read = read(&stream(), 1, &Cancel::new(), &some).unwrap().unwrap();
        assert_eq!(read.last().map(|c| c.2.as_str()), Some("下"));
    }
}

#[cfg(test)]
mod progressive_tests {
    use super::*;
    use nikomiru_subs::pgs::{parse, testing};

    fn cues() -> Vec<nikomiru_subs::pgs::PgsCue> {
        let mut bytes = Vec::new();
        for i in 0..40 {
            testing::display_set(
                &mut bytes,
                i * 2000,
                &[(100, 900, testing::block(80 + i as usize, 20))],
                255,
            );
            testing::display_set(&mut bytes, i * 2000 + 1000, &[], 255);
        }
        parse(&bytes)
    }

    // T-SUB-030: nearby/future pictures first, bounded cumulative progress,
    // with a final track identical to the former whole-track algorithm.
    #[test]
    fn progressive_batches_prioritise_the_playback_position_and_finish_whole() {
        let cues = cues();
        let snapshots = Mutex::new(Vec::new());
        let reader = |p: &Picture| Ok(p.width.to_string());
        let output = read_progressive(
            &cues,
            20_500,
            1,
            &Cancel::new(),
            &reader,
            &|text, done, total| {
                snapshots.lock().unwrap().push((text.to_vec(), done, total));
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            output,
            read(&cues, 1, &Cancel::new(), &reader).unwrap().unwrap()
        );
        let snapshots = snapshots.into_inner().unwrap();
        assert_eq!(snapshots[0].0[0].0, 20_000);
        assert_eq!(snapshots[0].1, BATCH);
        assert!(snapshots.iter().all(|(_, _, total)| *total == 40));
        assert_eq!(snapshots.last().unwrap().1, 40);
        assert!(snapshots.windows(2).all(|w| w[1].1 - w[0].1 <= BATCH));
        assert!(
            snapshots
                .iter()
                .all(|(text, _, _)| text.windows(2).all(|w| w[0].0 <= w[1].0))
        );
    }

    // T-SUB-030: no final result after cancellation from the first progress
    // callback; a failed batch need not discard later readable pictures.
    #[test]
    fn cancellation_and_failed_batches_never_return_a_partial_success() {
        let cues = cues();
        let cancel = Cancel::new();
        let reader = |p: &Picture| Ok(p.width.to_string());
        assert_eq!(
            read_progressive(&cues, 0, 1, &cancel, &reader, &|_, _, _| cancel.cancel()).unwrap(),
            None
        );
        let threshold = lines(&cues[BATCH - 1].picture().unwrap())[0][0].width;
        let broken = |p: &Picture| {
            if p.width <= threshold {
                Err("bad picture".into())
            } else {
                reader(p)
            }
        };
        let output = read_progressive(&cues, 0, 1, &Cancel::new(), &broken, &|_, _, _| {})
            .unwrap()
            .unwrap();
        assert_eq!(output.len(), 24);
        let failure = |_: &Picture| Err("no model".into());
        assert_eq!(
            read_progressive(&cues, 0, 1, &Cancel::new(), &failure, &|_, _, _| {}),
            Err("no model".into())
        );
    }
}
