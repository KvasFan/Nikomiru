// SPDX-License-Identifier: GPL-3.0-only

//! The media of an Anki card (ANK-033, ANK-034, ANK-042): the sentence audio
//! cut from the video with ffmpeg, the frame mpv saved, scaled and encoded,
//! or the sentence as an animated picture.
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

/// The longest animated picture, from the start of its sentence (ANK-042).
pub const MAX_ANIMATED_MS: i64 = 10_000;

/// How long encoding an animated picture may take.
pub const ANIMATED_TIMEOUT: Duration = Duration::from_secs(120);

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
        ImageFormat::Webp | ImageFormat::WebpAnimated => "webp",
        ImageFormat::AvifAnimated => "avif",
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

/// The long side and the frame rate of each level of an animated picture
/// (ANK-042): smaller than a frame's, a sentence is many frames.
const ANIMATED_SIDE: [u32; 4] = [480, 640, 854, 1280];
const ANIMATED_FPS: [u32; 4] = [10, 12, 15, 15];

/// The filter that scales to the long side `side`, never up.
fn scale(side: u32) -> String {
    format!("scale=w='if(gte(iw,ih),min({side},iw),-2)':h='if(gte(iw,ih),-2,min({side},ih))'")
}

/// The ffmpeg arguments that turn the frame `input` into the card image
/// `output` (ANK-034): scaled to the level's long side, never up, then encoded
/// at about q80 / q90 / q95 (level 3: original size, q95). An animated format
/// gives its still one.
pub fn image_args(input: &Path, output: &Path, format: ImageFormat, level: u8) -> Vec<OsString> {
    let level = level.min(3);
    let mut args = os(&["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]);
    args.push(file_arg(input));
    args.extend(os(&["-frames:v", "1"]));
    if let Some(&side) = LONG_SIDE.get(usize::from(level)) {
        args.extend(os(&["-vf", &scale(side)]));
    }
    match format.still() {
        ImageFormat::Jpeg => {
            let q = ["5", "3", "2", "2"][usize::from(level)];
            args.extend(os(&[
                "-c:v", "mjpeg", "-q:v", q, "-update", "1", "-f", "image2",
            ]));
        }
        _ => {
            let q = ["80", "90", "95", "95"][usize::from(level)];
            args.extend(os(&["-c:v", "libwebp", "-quality", q, "-f", "webp"]));
        }
    }
    args.push(file_arg(output));
    args
}

/// The ffmpeg arguments for the sentence `start_ms..end_ms` of `video` as an
/// animated picture (ANK-042): at most [`MAX_ANIMATED_MS`], without sound or
/// subtitles, at the level's long side and frame rate, looping. A still
/// format gives a WebP animation.
pub fn animated_args(
    video: &Path,
    (start_ms, end_ms): (i64, i64),
    output: &Path,
    format: ImageFormat,
    level: u8,
) -> Vec<OsString> {
    let level = usize::from(level.min(3));
    let start = start_ms.max(0);
    let duration = (end_ms - start).clamp(1, MAX_ANIMATED_MS);
    let mut args = os(&["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
    args.extend(os(&[
        "-ss",
        &seconds(start),
        "-t",
        &seconds(duration),
        "-i",
    ]));
    args.push(file_arg(video));
    let filter = format!(
        "fps={},{}",
        ANIMATED_FPS[level],
        scale(ANIMATED_SIDE[level])
    );
    args.extend(os(&["-map", "0:v:0", "-an", "-sn", "-dn", "-vf", &filter]));
    match format {
        ImageFormat::AvifAnimated => {
            let crf = ["42", "38", "34", "30"][level];
            args.extend(os(&[
                "-c:v",
                "libaom-av1",
                "-cpu-used",
                "8",
                "-row-mt",
                "1",
                "-crf",
                crf,
                "-pix_fmt",
                "yuv420p",
                "-f",
                "avif",
            ]));
        }
        _ => {
            let q = ["60", "70", "80", "85"][level];
            args.extend(os(&[
                "-c:v",
                "libwebp_anim",
                "-quality",
                q,
                "-loop",
                "0",
                "-f",
                "webp",
            ]));
        }
    }
    args.push(file_arg(output));
    args
}

/// The job of [`animated_args`], with `output` removed if it fails.
pub fn animated_job(
    ffmpeg: &Path,
    video: &Path,
    clip: (i64, i64),
    output: &Path,
    format: ImageFormat,
    level: u8,
) -> Job {
    let mut job = Job::new(ffmpeg, animated_args(video, clip, output, format, level));
    job.timeout = ANIMATED_TIMEOUT;
    job.outputs = vec![output.to_path_buf()];
    job
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

/// ffmpeg's arguments for the frame of `video` at `at_ms` as a PNG `height`
/// pixels high, without subtitles: a thumbnail of the progress bar
/// (PLY-014).
pub fn thumbnail_args(video: &Path, at_ms: i64, height: u32, output: &Path) -> Vec<OsString> {
    let mut args = frame_args(video, at_ms, output);
    // before the codec: `-vf scale=-2:<height>`
    let at = args.len() - 5;
    args.splice(
        at..at,
        os(&["-vf", &format!("scale=-2:{height}"), "-an", "-sn"]),
    );
    args
}

/// The job of [`thumbnail_args`].
pub fn thumbnail_job(ffmpeg: &Path, video: &Path, at_ms: i64, height: u32, output: &Path) -> Job {
    let mut job = Job::new(ffmpeg, thumbnail_args(video, at_ms, height, output));
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
