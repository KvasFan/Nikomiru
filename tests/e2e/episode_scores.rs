// SPDX-License-Identifier: GPL-3.0-only

//! KNW-008: embedded text scored before playback with the real word worker.

use super::support::{Desktop, exit_status, fixture};
use nikomiru_library::{Grouping, Library, SubtitleChoice};
use serde_json::json;
use std::time::Duration;

// T-KNW-008: Japanese embedded text beats an English file, a saved ASS track
// stays selected, Off/no text stay unscored, and statuses rescore every episode.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn embedded_episode_scores_match_playback_and_follow_word_statuses() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[player]\nmpv_options = { ao = \"null\" }\n[subtitles]\nword_colors = true\npreferred_languages = [\"ja\", \"en\"]\n");
    let root = desk.path("videos");
    let show = root.join("Show");
    std::fs::create_dir_all(&show).unwrap();
    let paths: Vec<_> = [
        "Show - 01.mkv",
        "Show - 02.mkv",
        "Show - 03.mkv",
        "Show - 04.mp4",
    ]
    .iter()
    .map(|name| show.join(name))
    .collect();
    for (path, source) in paths.iter().zip([
        "av_basic.mkv",
        "av_basic.mkv",
        "av_basic.mkv",
        "av_noaudio.mp4",
    ]) {
        std::os::unix::fs::symlink(fixture(source), path).unwrap();
    }
    std::fs::write(
        show.join("Show - 01.en.srt"),
        "1\n00:00:02,000 --> 00:00:02,500\n見る\n",
    )
    .unwrap();
    let id = {
        let lib = Library::open(&desk.data_file("library.redb")).unwrap();
        lib.import(
            &root,
            Grouping::ByFolder,
            &nikomiru_library::scan::video_files(&root),
            1,
        )
        .unwrap();
        let uid = lib.uid_for_path(&paths[1]).unwrap();
        let mut video = lib.video(&uid).unwrap().unwrap();
        let id = video.series.unwrap();
        video.subtitle = Some(SubtitleChoice::Embedded {
            ff_index: None,
            language: Some("jpn".into()),
            title: Some("ASS".into()),
        });
        lib.put_video(&uid, &video).unwrap();
        let uid = lib.uid_for_path(&paths[2]).unwrap();
        let mut video = lib.video(&uid).unwrap().unwrap();
        video.subtitle = Some(SubtitleChoice::Off);
        lib.put_video(&uid, &video).unwrap();
        id
    };
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"library", "action":"series", "id":id}));
    let s = h.state_until("embedded episodes counted before playback", |s| {
        let episodes = &s["library_page"]["open_series"]["episodes"];
        episodes.as_array().is_some_and(|e| e.len() == 4)
            && episodes[0]["score"].is_object()
            && episodes[1]["score"].is_object()
            && episodes[2]["score"].is_null()
            && episodes[3]["score"].is_null()
    });
    assert_eq!(s["loaded"], false);
    let first = s["library_page"]["open_series"]["episodes"][0]["score"].clone();
    let second = s["library_page"]["open_series"]["episodes"][1]["score"].clone();
    assert_eq!(
        first["lines"], 3,
        "Japanese embedded text, not the one-line English file: {s}"
    );
    assert_eq!(first["known"], 0);
    assert_eq!(second["lines"], 3);
    h.open(paths[0].to_str().unwrap());
    h.wait("file-loaded");
    h.state_until("the same subtitle score while playing", |s| {
        s["subtitles"]["source"] == "track:1" && s["subtitles"]["score"] == first
    });
    h.open(paths[1].to_str().unwrap());
    h.wait("file-loaded");
    h.state_until("the saved ASS track while playing", |s| {
        s["subtitles"]["source"] == "track:2" && s["subtitles"]["score"] == second
    });
    let words = desk.path("known.csv");
    std::fs::write(&words, "word,reading,status\n食べる,たべる,known\n").unwrap();
    h.cmd(json!({"cmd":"words", "action":"import", "path":words}));
    h.state_until("cached embedded counts rescored", |s| {
        let episodes = &s["library_page"]["open_series"]["episodes"];
        episodes[0]["score"]["known"] == 1
            && episodes[1]["score"]["known"] == 1
            && s["subtitles"]["score"]["known"] == 1
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
