// SPDX-License-Identifier: GPL-3.0-only

//! The fonts attached to a video file (SUB-025), as Fushi reads them
//! (`fushi/lib/src/media/video/subtitle_embedded_fonts.dart`): listed by
//! ffprobe, written by ffmpeg's `-dump_attachment` under names of our own,
//! and cached by the file's path, size and mtime, so a video is read once.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::extract::video_key;
use crate::process::{Cancel, Job, JobError, file_arg, output, run};

/// The most fonts taken from one file.
pub const MAX_FONTS: usize = 100;

/// The largest font kept.
pub const MAX_FONT_BYTES: u64 = 50 << 20;

/// How long ffprobe and ffmpeg may take: attachments sit at the start of a
/// Matroska file.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The file that marks a cache folder complete.
const DONE: &str = "done";

/// An attachment stream of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Its ff-index.
    pub index: i64,
    /// Its `filename` tag.
    pub filename: String,
    /// Its `mimetype` tag.
    pub mimetype: String,
}

impl Attachment {
    /// A font, by its MIME type or the end of its name.
    pub fn is_font(&self) -> bool {
        const TYPES: [&str; 10] = [
            "application/x-truetype-font",
            "application/x-font-ttf",
            "application/x-font-otf",
            "application/x-font-opentype",
            "application/vnd.ms-opentype",
            "application/font-sfnt",
            "font/ttf",
            "font/otf",
            "font/sfnt",
            "font/collection",
        ];
        let mime = self.mimetype.trim().to_ascii_lowercase();
        let name = self.filename.to_ascii_lowercase();
        TYPES.contains(&mime.as_str())
            || [".ttf", ".otf", ".ttc", ".otc"]
                .iter()
                .any(|ext| name.ends_with(ext))
    }

    /// The extension of the file written for it.
    fn extension(&self) -> &'static str {
        let name = self.filename.to_ascii_lowercase();
        if name.ends_with(".otf") {
            "otf"
        } else if name.ends_with(".ttc") || name.ends_with(".otc") {
            "ttc"
        } else {
            "ttf"
        }
    }
}

/// The ffprobe arguments that list the attachments of `video` as JSON.
pub fn probe_args(video: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-v",
        "error",
        "-select_streams",
        "t",
        "-show_entries",
        "stream=index,codec_type:stream_tags=filename,mimetype",
        "-of",
        "json",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(file_arg(video));
    args
}

/// The attachments in what ffprobe printed; nothing if it is not its JSON.
pub fn parse_probe(json: &str) -> Vec<Attachment> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let streams = value["streams"].as_array().cloned().unwrap_or_default();
    streams
        .iter()
        .filter(|s| s["codec_type"].as_str().is_none_or(|t| t == "attachment"))
        .filter_map(|s| {
            let tag = |name: &str| s["tags"][name].as_str().unwrap_or_default().to_string();
            Some(Attachment {
                index: s["index"].as_i64()?,
                filename: tag("filename"),
                mimetype: tag("mimetype"),
            })
        })
        .collect()
}

/// The ffmpeg arguments that write each attachment to its file, reading
/// nothing else of `video`.
pub fn dump_args(video: &Path, outputs: &[(i64, PathBuf)]) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]
        .iter()
        .map(OsString::from)
        .collect();
    for (index, out) in outputs {
        args.push(format!("-dump_attachment:{index}").into());
        args.push(out.as_os_str().to_os_string());
    }
    args.push("-i".into());
    args.push(file_arg(video));
    for a in ["-t", "0", "-f", "null", "-"] {
        args.push(a.into());
    }
    args
}

/// The font files of a complete cache folder, in order.
fn cached(dir: &Path) -> Option<Vec<PathBuf>> {
    if !dir.join(DONE).is_file() {
        return None;
    }
    let mut fonts: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n != DONE))
        .collect();
    fonts.sort();
    Some(fonts)
}

/// The fonts attached to `video`, written once to `cache_dir/fonts/<key>/`
/// and found there afterwards. A font ffmpeg did not write, or one too
/// large, is left out.
pub fn fonts(
    ffprobe: &Path,
    ffmpeg: &Path,
    video: &Path,
    cache_dir: &Path,
    cancel: &Cancel,
) -> Result<Vec<PathBuf>, JobError> {
    let spawn = |source: std::io::Error| JobError::Spawn {
        program: video.to_path_buf(),
        source,
    };
    let dir = cache_dir
        .join("fonts")
        .join(video_key(video).map_err(spawn)?);
    if let Some(fonts) = cached(&dir) {
        return Ok(fonts);
    }
    let probe = Job {
        timeout: TIMEOUT,
        ..Job::new(ffprobe, probe_args(video))
    };
    let attached: Vec<Attachment> = parse_probe(&output(&probe, cancel)?)
        .into_iter()
        .filter(Attachment::is_font)
        .take(MAX_FONTS)
        .collect();
    let mut part = dir.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(spawn)?;
    let outputs: Vec<(i64, PathBuf)> = attached
        .iter()
        .enumerate()
        .map(|(n, a)| (a.index, part.join(format!("{n:03}.{}", a.extension()))))
        .collect();
    if !outputs.is_empty() {
        let job = Job {
            timeout: TIMEOUT,
            ..Job::new(ffmpeg, dump_args(video, &outputs))
        };
        if let Err(e) = run(&job, cancel) {
            let _ = std::fs::remove_dir_all(&part);
            return Err(e);
        }
    }
    for (_, path) in &outputs {
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        if size == 0 || size > MAX_FONT_BYTES {
            let _ = std::fs::remove_file(path);
        }
    }
    std::fs::write(part.join(DONE), b"").map_err(spawn)?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&part, &dir).map_err(spawn)?;
    Ok(cached(&dir).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-025
    #[test]
    fn the_fonts_among_the_attachments() {
        let json = r#"{"streams": [
            {"index": 3, "codec_type": "attachment",
             "tags": {"filename": "Rounded.TTF", "mimetype": "application/octet-stream"}},
            {"index": 4, "codec_type": "attachment",
             "tags": {"filename": "a", "mimetype": "application/x-truetype-font"}},
            {"index": 5, "codec_type": "attachment",
             "tags": {"filename": "cover.jpg", "mimetype": "image/jpeg"}},
            {"index": 6, "codec_type": "attachment", "tags": {"filename": "b.otf"}},
            {"codec_type": "attachment", "tags": {"filename": "no-index.ttf"}}
        ]}"#;
        let attached = parse_probe(json);
        assert_eq!(attached.len(), 4);
        let fonts: Vec<i64> = attached
            .iter()
            .filter(|a| a.is_font())
            .map(|a| a.index)
            .collect();
        assert_eq!(fonts, [3, 4, 6]);
        assert_eq!(attached[3].extension(), "otf");
        assert!(parse_probe("not json").is_empty());
        assert!(parse_probe("{}").is_empty());
        let args = dump_args(
            Path::new("-v.mkv"),
            &[
                (3, PathBuf::from("/c/000.ttf")),
                (6, PathBuf::from("/c/001.otf")),
            ],
        );
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-dump_attachment:3",
                "/c/000.ttf",
                "-dump_attachment:6",
                "/c/001.otf",
                "-i",
                "file:-v.mkv",
                "-t",
                "0",
                "-f",
                "null",
                "-"
            ]
        );
        assert_eq!(
            probe_args(Path::new("/v.mkv")).last().unwrap(),
            "file:/v.mkv"
        );
    }
}
