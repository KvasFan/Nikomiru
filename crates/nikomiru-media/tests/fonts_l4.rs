// SPDX-License-Identifier: GPL-3.0-only

//! L4: the fonts attached to a real Matroska file, through ffprobe and
//! ffmpeg (T-SUB-025).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use nikomiru_media::attachments;
use nikomiru_media::process::Cancel;
use nikomiru_media::tools::{Search, probe};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

/// A font file of this system, through fontconfig.
fn system_font() -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "sans"])
        .output()
        .ok()?;
    let path = PathBuf::from(String::from_utf8(out.stdout).ok()?);
    path.is_file().then_some(path)
}

// T-SUB-025: written once, the same bytes; from the cache afterwards; none
// for a file without fonts
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_fonts_attached_to_a_video_are_written_once() {
    let tools = probe(&Search::from_env("")).unwrap();
    let Some(font) = system_font() else {
        eprintln!("no font on this system");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("with font.mkv");
    let status = std::process::Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
        .arg(fixture("av_basic.mkv"))
        .arg("-attach")
        .arg(&font)
        .args([
            "-metadata:s:t",
            "mimetype=application/x-truetype-font",
            "-c",
            "copy",
        ])
        .arg(&video)
        .status()
        .unwrap();
    assert!(status.success());
    let cache = dir.path().join("cache");
    let fonts = attachments::fonts(
        &tools.ffprobe,
        &tools.ffmpeg,
        &video,
        &cache,
        &Cancel::new(),
    )
    .unwrap();
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    assert_eq!(
        std::fs::read(&fonts[0]).unwrap(),
        std::fs::read(&font).unwrap()
    );
    // the second time no program runs
    let again = attachments::fonts(
        Path::new("/nonexistent/ffprobe"),
        Path::new("/nonexistent/ffmpeg"),
        &video,
        &cache,
        &Cancel::new(),
    )
    .unwrap();
    assert_eq!(again, fonts);
    let none = attachments::fonts(
        &tools.ffprobe,
        &tools.ffmpeg,
        &fixture("av_basic.mkv"),
        &cache,
        &Cancel::new(),
    )
    .unwrap();
    assert!(none.is_empty());
    let left: Vec<_> = std::fs::read_dir(cache.join("fonts"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| n.to_string_lossy().ends_with(".part"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}
