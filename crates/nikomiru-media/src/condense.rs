// SPDX-License-Identifier: GPL-3.0-only

//! Condensed episode audio and chapters (AUD-010). Segments are decoded to
//! PCM, joined, then encoded once, so joins add no codec delay per line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::process::{Cancel, Job, file_arg, run};

/// The export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Ogg Opus, with Vorbis chapter tags.
    Opus,
    /// MP3, with ID3 chapters.
    Mp3,
}

impl Format {
    /// The output extension.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Mp3 => "mp3",
        }
    }
}

/// A subtitle line after padding and offset, in source milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Padded start.
    pub start_ms: i64,
    /// Padded end.
    pub end_ms: i64,
    /// Chapter title.
    pub text: String,
}

/// The source intervals retained and chapter starts in the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Sorted, disjoint intervals, with small gaps retained.
    pub spans: Vec<(i64, i64)>,
    /// Starts in condensed milliseconds, and titles.
    pub chapters: Vec<(i64, String)>,
}

impl Plan {
    /// Clamp lines to the media, merge gaps under one second and translate
    /// each chapter to the condensed timeline. Empty/invalid lines are omitted.
    pub fn new(lines: &[Line], duration_ms: i64) -> Self {
        let mut lines: Vec<_> = lines
            .iter()
            .filter_map(|line| {
                let start = line.start_ms.clamp(0, duration_ms.max(0));
                let end = line.end_ms.clamp(0, duration_ms.max(0));
                (start < end && !line.text.trim().is_empty()).then_some((start, end, &line.text))
            })
            .collect();
        lines.sort_by_key(|(start, end, _)| (*start, *end));
        let mut spans: Vec<(i64, i64)> = Vec::new();
        for &(start, end, _) in &lines {
            if let Some(last) = spans.last_mut()
                && start.saturating_sub(last.1) < 1000
            {
                last.1 = last.1.max(end);
            } else {
                spans.push((start, end));
            }
        }
        let mut chapters: Vec<(i64, String)> = Vec::new();
        let (mut span, mut elapsed) = (0, 0);
        for (start, _, text) in lines {
            while span < spans.len() && start >= spans[span].1 {
                elapsed += spans[span].1 - spans[span].0;
                span += 1;
            }
            if let Some(&(from, _)) = spans.get(span) {
                let at = elapsed + start - from;
                if let Some(last) = chapters.last_mut()
                    && last.0 == at
                {
                    last.1.push_str(" / ");
                    last.1.push_str(text);
                } else {
                    chapters.push((at, text.clone()));
                }
            }
        }
        Self { spans, chapters }
    }

    /// The duration of the condensed result.
    pub fn duration_ms(&self) -> i64 {
        self.spans.iter().map(|(a, b)| b - a).sum()
    }

    /// ffmetadata; Ogg uses explicit chapter tags to avoid muxer-dependent
    /// timestamp rescaling. MP3 uses native chapter sections.
    pub fn metadata(&self, format: Format) -> String {
        let mut out = ";FFMETADATA1\n".to_string();
        for (i, (start, title)) in self.chapters.iter().enumerate() {
            let title = title
                .replace('\\', "\\\\")
                .replace('=', "\\=")
                .replace(';', "\\;")
                .replace('#', "\\#")
                .replace(['\r', '\n'], " ");
            match format {
                Format::Opus => out.push_str(&format!(
                    "CHAPTER{i:03}={:02}:{:02}:{:02}.{:03}\nCHAPTER{i:03}NAME={title}\n",
                    start / 3_600_000,
                    start / 60_000 % 60,
                    start / 1000 % 60,
                    start % 1000
                )),
                Format::Mp3 => {
                    let end = self.chapters.get(i + 1).map_or(self.duration_ms(), |c| c.0);
                    out.push_str(&format!(
                        "[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={end}\ntitle={title}\n"
                    ));
                }
            }
        }
        out
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(folder: &Path) -> std::io::Result<Scratch> {
    loop {
        let p = folder.join(format!(
            ".nikomiru-audio-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(Scratch(p)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

fn reserve(folder: &Path, name: &str, format: Format) -> std::io::Result<PathBuf> {
    for n in 0..10_000 {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let p = folder.join(format!("{name} condensed{suffix}.{}", format.extension()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(_) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no available export name",
    ))
}

/// Decode each merged span accurately, encode the result once, and publish a
/// new file in `folder`. The caller supplies a nonempty plan and a file stem.
/// Cancellation and failures remove every intermediate and reserved output.
pub fn export(
    ffmpeg: &Path,
    input: &Path,
    stream: Option<i64>,
    plan: &Plan,
    folder: &Path,
    format: Format,
    cancel: &Cancel,
) -> Result<PathBuf, String> {
    if plan.spans.is_empty() {
        return Err("no subtitle audio to export".into());
    }
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let work = scratch(folder).map_err(|e| e.to_string())?;
    let seconds = |ms: i64| format!("{}.{:03}", ms / 1000, ms % 1000);
    let mut concat = "ffconcat version 1.0\n".to_string();
    for (i, &(start, end)) in plan.spans.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let name = format!("{i:06}.wav");
        let chunk = work.0.join(&name);
        let mut job = Job::new(
            ffmpeg,
            [
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-ss",
                &seconds(start),
                "-i",
            ],
        );
        job.args.push(file_arg(input));
        job.args.extend([
            "-t".into(),
            seconds(end - start).into(),
            "-map".into(),
            stream.map_or_else(|| "0:a:0".into(), |s| format!("0:{s}").into()),
        ]);
        job.args.extend(
            [
                "-vn",
                "-sn",
                "-dn",
                "-map_chapters",
                "-1",
                "-c:a",
                "pcm_s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-f",
                "wav",
            ]
            .map(Into::into),
        );
        job.args.push(file_arg(&chunk));
        job.outputs.push(chunk);
        job.timeout = Duration::from_secs(120 + (end - start) as u64 / 1000);
        run(&job, cancel).map_err(|e| e.to_string())?;
        concat.push_str(&format!("file '{name}'\n"));
    }
    let list = work.0.join("audio.ffconcat");
    let meta = work.0.join("chapters.ffmeta");
    std::fs::write(&list, concat)
        .and_then(|_| std::fs::write(&meta, plan.metadata(format)))
        .map_err(|e| e.to_string())?;
    let encoded = work.0.join(format!("result.{}", format.extension()));
    let mut job = Job::new(
        ffmpeg,
        [
            "-nostdin", "-v", "error", "-y", "-f", "concat", "-safe", "1", "-i",
        ],
    );
    job.args.push(file_arg(&list));
    job.args.extend(["-f", "ffmetadata", "-i"].map(Into::into));
    job.args.push(file_arg(&meta));
    job.args.extend(
        [
            "-map",
            "0:a:0",
            "-map_metadata",
            "1",
            "-map_chapters",
            if format == Format::Opus { "-1" } else { "1" },
            "-c:a",
            if format == Format::Opus {
                "libopus"
            } else {
                "libmp3lame"
            },
            "-b:a",
            "128k",
        ]
        .map(Into::into),
    );
    job.args.push(file_arg(&encoded));
    job.outputs.push(encoded.clone());
    job.timeout = Duration::from_secs(120 + plan.duration_ms() as u64 / 1000);
    run(&job, cancel).map_err(|e| e.to_string())?;
    if cancel.is_cancelled() {
        return Err("cancelled".into());
    }
    let name = input.file_stem().unwrap_or_default().to_string_lossy();
    let target = reserve(folder, &name, format).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&encoded, &target) {
        let _ = std::fs::remove_file(&target);
        return Err(e.to_string());
    }
    Ok(target)
}
