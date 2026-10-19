// SPDX-License-Identifier: GPL-3.0-only

//! Extraction of embedded text subtitle tracks (SUB-007): one ffmpeg pass for
//! every requested track, then track by track if that pass fails. The results
//! are cached by (path, size, mtime, ff-index): a second request starts no
//! process.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::process::{Cancel, Job, JobError, file_arg, run};

/// A text subtitle stream of a video file, as mpv lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    /// FFmpeg stream index (mpv `ff-index`).
    pub ff_index: i64,
    /// Codec name (mpv `codec`: `subrip`, `ass`, `webvtt`, `mov_text`…).
    pub codec: String,
}

/// The format a stream is extracted to: ASS keeps its styles, WebVTT stays
/// WebVTT, every other text codec becomes SRT.
pub fn target(codec: &str) -> (&'static str, &'static str) {
    match codec {
        "ass" | "ssa" => ("ass", "ass"),
        "webvtt" => ("webvtt", "vtt"),
        _ => ("srt", "srt"),
    }
}

/// Whether mpv's codec name is a text subtitle codec Nikomiru can draw.
pub fn is_text_codec(codec: &str) -> bool {
    matches!(
        codec,
        "subrip" | "srt" | "ass" | "ssa" | "webvtt" | "mov_text" | "text" | "microdvd" | "eia_608"
    )
}

/// Extracts and caches subtitle streams.
#[derive(Debug, Clone)]
pub struct Extractor {
    /// The ffmpeg executable.
    pub ffmpeg: PathBuf,
    /// Cache folder.
    pub cache_dir: PathBuf,
}

/// The extraction timeout: 60 s plus one second per minute of video (EXT-003).
pub fn timeout_for(duration: Duration) -> Duration {
    Duration::from_secs(60 + duration.as_secs() / 60)
}

impl Extractor {
    /// Cache file of `stream` in `video`.
    pub fn cache_path(&self, video: &Path, stream: &Stream) -> std::io::Result<PathBuf> {
        let meta = std::fs::metadata(video)?;
        let mtime = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let mut h = Sha256::new();
        h.update(video.as_os_str().as_encoded_bytes());
        h.update(meta.len().to_le_bytes());
        h.update(mtime.to_le_bytes());
        let key: String = h.finalize()[..12]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let (_, ext) = target(&stream.codec);
        Ok(self
            .cache_dir
            .join(format!("{key}-{}.{ext}", stream.ff_index)))
    }

    /// The ffmpeg arguments that write each stream to its output (T-EXT-002).
    pub fn args(video: &Path, outputs: &[(&Stream, PathBuf)]) -> Vec<OsString> {
        let mut args: Vec<OsString> =
            ["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]
                .iter()
                .map(OsString::from)
                .collect();
        args.push(file_arg(video));
        for (stream, out) in outputs {
            let (muxer, _) = target(&stream.codec);
            args.push("-map".into());
            args.push(format!("0:{}", stream.ff_index).into());
            args.push("-c:s".into());
            args.push(muxer.into());
            args.push("-f".into());
            args.push(muxer.into());
            args.push(file_arg(out));
        }
        args
    }

    fn pass(
        &self,
        video: &Path,
        streams: &[&Stream],
        timeout: Duration,
        cancel: &Cancel,
    ) -> Result<Vec<(i64, PathBuf)>, JobError> {
        let mut outputs = Vec::new();
        for s in streams {
            let path = self
                .cache_path(video, s)
                .map_err(|source| JobError::Spawn {
                    program: video.to_path_buf(),
                    source,
                })?;
            let mut part = path.clone().into_os_string();
            part.push(".part");
            outputs.push((*s, path, PathBuf::from(part)));
        }
        let pairs: Vec<(&Stream, PathBuf)> = outputs
            .iter()
            .map(|(s, _, part)| (*s, part.clone()))
            .collect();
        let job = Job {
            program: self.ffmpeg.clone(),
            args: Self::args(video, &pairs),
            timeout,
            outputs: outputs.iter().map(|(_, _, part)| part.clone()).collect(),
        };
        run(&job, cancel)?;
        let mut done = Vec::new();
        for (s, path, part) in outputs {
            if std::fs::rename(&part, &path).is_ok() {
                done.push((s.ff_index, path));
            }
        }
        Ok(done)
    }

    /// Extracts `streams` of `video` (or finds them in the cache) and returns
    /// each stream's file. Streams that fail are left out; the error of the
    /// last failure is returned only if none succeeded.
    pub fn extract(
        &self,
        video: &Path,
        streams: &[Stream],
        duration: Duration,
        cancel: &Cancel,
    ) -> Result<Vec<(i64, PathBuf)>, JobError> {
        let spawn_error = |source| JobError::Spawn {
            program: self.cache_dir.clone(),
            source,
        };
        std::fs::create_dir_all(&self.cache_dir).map_err(spawn_error)?;
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for s in streams {
            let path = self
                .cache_path(video, s)
                .map_err(|source| JobError::Spawn {
                    program: video.to_path_buf(),
                    source,
                })?;
            if path.is_file() {
                found.push((s.ff_index, path));
            } else {
                missing.push(s);
            }
        }
        if missing.is_empty() {
            return Ok(found);
        }
        let timeout = timeout_for(duration);
        match self.pass(video, &missing, timeout, cancel) {
            Ok(done) => found.extend(done),
            Err(e @ JobError::Cancelled { .. }) => return Err(e),
            Err(first) => {
                tracing::warn!("subtitle extraction in one pass failed, track by track: {first}");
                let mut last = first;
                for s in missing {
                    match self.pass(video, &[s], timeout, cancel) {
                        Ok(done) => found.extend(done),
                        Err(e @ JobError::Cancelled { .. }) => return Err(e),
                        Err(e) => last = e,
                    }
                }
                if found.is_empty() {
                    return Err(last);
                }
            }
        }
        found.sort_by_key(|(i, _)| streams.iter().position(|s| s.ff_index == *i));
        Ok(found)
    }
}
