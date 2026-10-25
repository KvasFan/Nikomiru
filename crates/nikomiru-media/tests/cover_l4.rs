// SPDX-License-Identifier: GPL-3.0-only

//! L4: the cover of a real file through ffprobe and ffmpeg (T-LIB-005).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use nikomiru_media::cover;
use nikomiru_media::process::Cancel;
use nikomiru_media::tools::{Search, probe};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

// T-LIB-005: made once at 10%, then served from the cache
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn a_cover_is_made_once_and_kept() {
    let tools = probe(&Search::from_env("")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = cover::path(dir.path(), "video/av_basic");
    cover::make(
        &tools.ffmpeg,
        &tools.ffprobe,
        &fixture("av_basic.mkv"),
        &out,
        &Cancel::new(),
    )
    .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..3], [0xff, 0xd8, 0xff], "a JPEG");
    assert!(!out.with_extension("part.jpg").exists());
    // the second time no program runs: a missing one does not matter
    cover::make(
        Path::new("/nonexistent/ffmpeg"),
        Path::new("/nonexistent/ffprobe"),
        &fixture("av_basic.mkv"),
        &out,
        &Cancel::new(),
    )
    .unwrap();
    // a file that is not a video: an error, and no cover left behind
    let bad = cover::path(dir.path(), "video/bad");
    let text = dir.path().join("not a video.mkv");
    std::fs::write(&text, b"hello").unwrap();
    assert!(cover::make(&tools.ffmpeg, &tools.ffprobe, &text, &bad, &Cancel::new()).is_err());
    assert!(!bad.exists() && !bad.with_extension("part.jpg").exists());
}
