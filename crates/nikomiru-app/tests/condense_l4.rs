// SPDX-License-Identifier: GPL-3.0-only

//! AUD-010: a whole series, external and embedded subtitles, saved choices,
//! a broken episode and cancellation, on real ffmpeg.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_core::{Cancel, Config};
use nikomiru_library::{Grouping, Library, SubtitleChoice};
use nikomiru_media::{
    condense::Format,
    tools::{Search, find},
};
use nikomiru_ui::{
    condense::{Request, Target},
    i18n::Translator,
};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

// T-AUD-010
#[test]
#[ignore = "L4: needs ffmpeg and cargo xtask fixtures"]
fn a_series_exports_external_and_embedded_tracks_and_reports_each_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("videos");
    std::fs::create_dir(&root).unwrap();
    let paths: Vec<_> = ["ep01.mkv", "ep02.mkv", "ep03.mp4"]
        .map(|n| root.join(n))
        .into();
    for (path, source) in paths
        .iter()
        .zip(["av_basic.mkv", "av_basic.mkv", "av_noaudio.mp4"])
    {
        std::fs::copy(fixture(source), path).unwrap();
    }
    let custom = root.join("chosen.srt");
    std::fs::write(&custom, "1\n00:00:02,000 --> 00:00:02,500\n選択\n").unwrap();
    let lib = Library::open(&dir.path().join("library.redb")).unwrap();
    lib.import(
        &root,
        Grouping::ByFolder,
        &nikomiru_library::scan::video_files(&root),
        1,
    )
    .unwrap();
    let uid = lib.uid_for_path(&paths[0]).unwrap();
    let mut video = lib.video(&uid).unwrap().unwrap();
    video.subtitle = Some(SubtitleChoice::File(custom.to_string_lossy().into()));
    video.delay_ms = Some(300);
    video.aid = Some(2);
    lib.put_video(&uid, &video).unwrap();
    let id = video.series.unwrap();
    let request = Request {
        target: Target::Series(id),
        current: None,
        folder: dir.path().join("exports"),
        format: Format::Opus,
        config: Box::new(Config::default()),
        subtitles_dir: dir.path().join("subtitles"),
        cancel: Cancel::new(),
    };
    let (_, ffmpeg, ffprobe) = find(&Search::from_env("")).unwrap();
    let tr = Translator::from_setting("en", |_| None);
    let report = nikomiru_app::condense::run(
        &lib,
        &request,
        &ffmpeg,
        &ffprobe,
        &dir.path().join("cache"),
        &tr,
    )
    .unwrap();
    assert_eq!(report.written.len(), 2, "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert!(report.failed[0].0.ends_with("ep03.mp4"));
    for path in &report.written {
        assert!(std::fs::metadata(path).unwrap().len() > 100);
    }
    request.cancel.cancel();
    let cancelled = nikomiru_app::condense::run(
        &lib,
        &request,
        &ffmpeg,
        &ffprobe,
        &dir.path().join("cache"),
        &tr,
    )
    .unwrap();
    assert!(cancelled.written.is_empty());
    assert_eq!(std::fs::read_dir(&request.folder).unwrap().count(), 2);
}
