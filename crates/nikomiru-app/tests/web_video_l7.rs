// SPDX-License-Identifier: GPL-3.0-only

//! PLY-023: opt-in smoke check of real yt-dlp, Japanese subtitles and CDN
//! audio/pictures through the same relay used by the app. No video download.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_core::Cancel;
use nikomiru_media::{
    clip::{AudioClip, frame_job},
    process::run,
    tools::{AudioFormat, Search, find},
};
use nikomiru_ui::{i18n::Translator, web_video::Request};
use std::path::PathBuf;

// T-PLY-023
#[test]
#[ignore = "L7: set NIKOMIRU_TEST_WEB_VIDEO_URL and NIKOMIRU_TEST_YT_DLP for a public Japanese video"]
fn l7_real_web_video_has_japanese_text_audio_and_picture() {
    let page =
        std::env::var("NIKOMIRU_TEST_WEB_VIDEO_URL").expect("set NIKOMIRU_TEST_WEB_VIDEO_URL");
    let program = std::env::var("NIKOMIRU_TEST_YT_DLP").expect("set NIKOMIRU_TEST_YT_DLP");
    let dir = tempfile::tempdir().unwrap();
    let tr = Translator::from_setting("en", |_| None);
    let request = Request {
        generation: 1,
        page,
        executable: program,
        proxy: String::new(),
        languages: vec!["ja".into()],
        folder: dir.path().join("subs"),
        cancel: Cancel::new(),
    };
    let video = nikomiru_app::web_video::resolve(&request, &tr).unwrap();
    let subtitle = video
        .subtitles
        .iter()
        .find(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().contains(".ja"))
        })
        .expect("the public video must have Japanese subtitles");
    let loaded = nikomiru_app::subtitles::read_file(subtitle, None, &tr).unwrap();
    let cue = loaded
        .track
        .cues
        .first()
        .expect("at least one subtitle line");
    let (_, ffmpeg, _) = find(&Search::from_env("")).unwrap();
    let image = dir.path().join("frame.png");
    run(
        &frame_job(&ffmpeg, &PathBuf::from(&video.video), cue.start_ms, &image),
        &Cancel::new(),
    )
    .unwrap();
    assert!(
        std::fs::read(&image)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    let audio = dir.path().join("sentence.m4a");
    run(
        &AudioClip {
            input: PathBuf::from(video.audio.as_ref().unwrap_or(&video.video)),
            stream: None,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            format: AudioFormat::AacM4a,
            quality: 1,
        }
        .job(&ffmpeg, &audio),
        &Cancel::new(),
    )
    .unwrap();
    assert!(std::fs::metadata(&audio).unwrap().len() > 100);
    assert_eq!(video.uid, nikomiru_online::web_video::uid(&request.page));
    eprintln!(
        "Real web video: {:?}; Japanese cues: {}; sentence audio and frame extracted",
        video.title,
        loaded.track.cues.len()
    );
}
