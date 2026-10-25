// SPDX-License-Identifier: GPL-3.0-only

//! The cover of a series until metadata give one (LIB-005): the frame at
//! 10% of its first episode, made once and kept in the cache.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process::{Cancel, Job, JobError, file_arg, output, run};

/// Where in the episode the cover is taken, in percent.
pub const AT_PERCENT: u64 = 10;

/// The cover's width; the height keeps the video's shape.
pub const WIDTH: u32 = 480;

/// How long probing or taking the frame may last.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The ffprobe arguments that print the duration of `input` in seconds.
pub fn duration_args(input: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(file_arg(input));
    args
}

/// The duration ffprobe printed; `None` for `N/A` or nothing.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let seconds: f64 = text.lines().next()?.trim().parse().ok()?;
    (seconds.is_finite() && seconds > 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// The ffmpeg arguments that write the frame at `at` of `input` as a JPEG
/// [`WIDTH`] wide at `output`.
pub fn frame_args(input: &Path, at: Duration, output: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-ss"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(format!("{:.3}", at.as_secs_f64()).into());
    args.push("-i".into());
    args.push(file_arg(input));
    args.extend(
        ["-map", "0:v:0", "-frames:v", "1", "-vf"]
            .iter()
            .map(OsString::from),
    );
    args.push(format!("scale={WIDTH}:-2").into());
    args.extend(["-q:v", "4", "-y"].iter().map(OsString::from));
    args.push(file_arg(output));
    args
}

/// The cached cover of the video `key` (a library uid) in `cache_dir`.
pub fn path(cache_dir: &Path, key: &str) -> PathBuf {
    let name: String = key
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    cache_dir.join("covers").join(format!("{name}.jpg"))
}

/// Makes the cover of `input` at `output`, unless it is there already:
/// ffprobe for the duration, ffmpeg for the frame at 10% (the first frame
/// when the duration is unknown).
pub fn make(
    ffmpeg: &Path,
    ffprobe: &Path,
    input: &Path,
    output_path: &Path,
    cancel: &Cancel,
) -> Result<(), JobError> {
    if output_path.is_file() {
        return Ok(());
    }
    let probe = Job {
        timeout: TIMEOUT,
        ..Job::new(ffprobe, duration_args(input))
    };
    let at = output(&probe, cancel)
        .ok()
        .and_then(|text| parse_duration(&text))
        .map_or(Duration::ZERO, |d| d * AT_PERCENT as u32 / 100);
    if let Some(dir) = output_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // written beside, then renamed: a cover is never half there
    let partial = output_path.with_extension("part.jpg");
    let job = Job {
        timeout: TIMEOUT,
        outputs: vec![partial.clone()],
        ..Job::new(ffmpeg, frame_args(input, at, &partial))
    };
    run(&job, cancel)?;
    std::fs::rename(&partial, output_path).map_err(|e| JobError::Failed {
        program: ffmpeg.to_path_buf(),
        status: "rename".into(),
        stderr: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LIB-005
    #[test]
    fn the_cover_is_the_frame_at_ten_percent() {
        assert_eq!(
            parse_duration("1440.250000\n"),
            Some(Duration::from_millis(1_440_250))
        );
        assert_eq!(parse_duration("N/A\n"), None);
        assert_eq!(parse_duration(""), None);
        let args: Vec<String> = frame_args(
            Path::new("-a.mkv"),
            Duration::from_millis(144_025),
            Path::new("/c/x.jpg"),
        )
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
        assert_eq!(
            args.join(" "),
            "-nostdin -hide_banner -loglevel error -ss 144.025 -i file:-a.mkv -map 0:v:0 -frames:v 1 -vf scale=480:-2 -q:v 4 -y file:/c/x.jpg"
        );
        assert_eq!(
            path(Path::new("/cache"), "video/Show - 01"),
            Path::new("/cache/covers/video_Show - 01.jpg")
        );
    }
}
