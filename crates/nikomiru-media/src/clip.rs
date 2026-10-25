// SPDX-License-Identifier: GPL-3.0-only

//! The media of an Anki card (ANK-033, ANK-034): the sentence audio cut from
//! the video with ffmpeg, and the frame mpv saved, scaled and encoded.
//!
//! Translated from Fushi `packages/fushi_engine/lib/utils/misc/desktop_audio_clipper.dart`
//! (commit `ac8034e`), with the changes of ADR 0007 §18–§19: the track is
//! chosen by its ff-index, M4A instead of raw AAC, the quality levels of the
//! settings.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process::{Job, file_arg};
use crate::tools::{AudioFormat, ImageFormat};

/// How long cutting the sentence audio may take.
pub const AUDIO_TIMEOUT: Duration = Duration::from_secs(120);

/// How long encoding the image may take.
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// The longest sentence audio (ANK-032).
pub const MAX_AUDIO_MS: i64 = 30_000;

/// The sentence audio to cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioClip {
    /// The video (or the external audio file).
    pub input: PathBuf,
    /// The ff-index of the audio stream; `None` takes the first one.
    pub stream: Option<i64>,
    /// Start, in milliseconds of the input.
    pub start_ms: i64,
    /// End.
    pub end_ms: i64,
    /// The format.
    pub format: AudioFormat,
    /// 0 = mono 64k, 1 = stereo 128k, 2 = stereo 192k.
    pub quality: u8,
}

/// The file extension of an audio format.
pub fn audio_extension(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::AacM4a => "m4a",
        AudioFormat::OpusOgg => "ogg",
        AudioFormat::Mp3 => "mp3",
    }
}

/// The file extension of an image format.
pub fn image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
    }
}

fn seconds(ms: i64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

impl AudioClip {
    /// The ffmpeg arguments that write it to `output` (ANK-033): `-ss` before
    /// `-i`, at most [`MAX_AUDIO_MS`].
    pub fn args(&self, output: &Path) -> Vec<OsString> {
        let start = self.start_ms.max(0);
        let duration = (self.end_ms - start).clamp(0, MAX_AUDIO_MS);
        let (channels, bitrate) = match self.quality {
            0 => ("1", "64k"),
            1 => ("2", "128k"),
            _ => ("2", "192k"),
        };
        let mut args = os(&["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
        args.extend(os(&[
            "-ss",
            &seconds(start),
            "-t",
            &seconds(duration),
            "-i",
        ]));
        args.push(file_arg(&self.input));
        args.extend(os(&["-vn", "-sn", "-map_chapters", "-1", "-map"]));
        args.push(match self.stream {
            Some(index) => format!("0:{index}").into(),
            None => "0:a:0".into(),
        });
        let (codec, muxer) = match self.format {
            AudioFormat::AacM4a => ("aac", "ipod"),
            AudioFormat::OpusOgg => ("libopus", "ogg"),
            AudioFormat::Mp3 => ("libmp3lame", "mp3"),
        };
        args.extend(os(&[
            "-c:a", codec, "-ac", channels, "-b:a", bitrate, "-f", muxer,
        ]));
        args.push(file_arg(output));
        args
    }

    /// The job, with `output` removed if it fails.
    pub fn job(&self, ffmpeg: &Path, output: &Path) -> Job {
        let mut job = Job::new(ffmpeg, self.args(output));
        job.timeout = AUDIO_TIMEOUT;
        job.outputs = vec![output.to_path_buf()];
        job
    }
}

/// The long side of each image level (ANK-034); level 3 keeps the original.
const LONG_SIDE: [u32; 3] = [720, 1000, 2000];

/// The ffmpeg arguments that turn the frame `input` into the card image
/// `output` (ANK-034): scaled to the level's long side, never up, then encoded
/// at about q80 / q90 / q95 (level 3: original size, q95).
pub fn image_args(input: &Path, output: &Path, format: ImageFormat, level: u8) -> Vec<OsString> {
    let level = level.min(3);
    let mut args = os(&["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]);
    args.push(file_arg(input));
    args.extend(os(&["-frames:v", "1"]));
    if let Some(&side) = LONG_SIDE.get(usize::from(level)) {
        let scale = format!(
            "scale=w='if(gte(iw,ih),min({side},iw),-2)':h='if(gte(iw,ih),-2,min({side},ih))'"
        );
        args.extend(os(&["-vf", &scale]));
    }
    match format {
        ImageFormat::Jpeg => {
            let q = ["5", "3", "2", "2"][usize::from(level)];
            args.extend(os(&[
                "-c:v", "mjpeg", "-q:v", q, "-update", "1", "-f", "image2",
            ]));
        }
        ImageFormat::Webp => {
            let q = ["80", "90", "95", "95"][usize::from(level)];
            args.extend(os(&["-c:v", "libwebp", "-quality", q, "-f", "webp"]));
        }
    }
    args.push(file_arg(output));
    args
}

/// ffmpeg's arguments for the frame of `video` at `at_ms` as a PNG of its
/// own size, without subtitles: the picture of a card taken at the start of
/// its sentence (ANK-040).
pub fn frame_args(video: &Path, at_ms: i64, output: &Path) -> Vec<OsString> {
    let mut args = os(&[
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-ss",
    ]);
    args.push(format!("{:.3}", at_ms.max(0) as f64 / 1000.0).into());
    args.push("-i".into());
    args.push(file_arg(video));
    args.extend(os(&[
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-c:v",
        "png",
        "-f",
        "image2",
    ]));
    args.push(file_arg(output));
    args
}

/// The job of [`frame_args`].
pub fn frame_job(ffmpeg: &Path, video: &Path, at_ms: i64, output: &Path) -> Job {
    let mut job = Job::new(ffmpeg, frame_args(video, at_ms, output));
    job.timeout = IMAGE_TIMEOUT;
    job.outputs = vec![output.to_path_buf()];
    job
}

/// The image job, with `output` removed if it fails.
pub fn image_job(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    format: ImageFormat,
    level: u8,
) -> Job {
    let mut job = Job::new(ffmpeg, image_args(input, output, format, level));
    job.timeout = IMAGE_TIMEOUT;
    job.outputs = vec![output.to_path_buf()];
    job
}
