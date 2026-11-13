// SPDX-License-Identifier: GPL-3.0-only

//! KNW-017: weighted series cards, actual external/embedded subtitles.

use super::support::{Desktop, exit_status, fixture};
use nikomiru_library::{Grouping, Library, SubtitleChoice};
use serde_json::{Value, json};
use std::time::Duration;

fn score(s: &Value) -> &Value {
    &s["library_page"]["series"][0]["score"]
}

// T-KNW-017: count without opening/playing, exact episode totals, coverage,
// shared status updates and explicit refresh without changing the open view.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn a_series_card_counts_local_subtitles_and_keeps_source_coverage() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors = true\npreferred_languages = [\"ja\"]\n");
    let root = desk.path("videos");
    let show = root.join("Show");
    std::fs::create_dir_all(&show).unwrap();
    let paths: Vec<_> = (1..=4)
        .map(|i| show.join(format!("Show - {i:02}.mkv")))
        .collect();
    for (i, path) in paths.iter().enumerate() {
        std::os::unix::fs::symlink(
            fixture(if i == 0 || i == 3 {
                "av_noaudio.mp4"
            } else {
                "av_basic.mkv"
            }),
            path,
        )
        .unwrap();
    }
    let sidecar = show.join("Show - 01.ja.srt");
    std::fs::write(
        &sidecar,
        format!(
            "1\n00:00:01,000 --> 00:00:02,000\n{}\n",
            "食べる。".repeat(9)
        ),
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
        let uid = lib.uid_for_path(&paths[2]).unwrap();
        let mut video = lib.video(&uid).unwrap().unwrap();
        video.subtitle = Some(SubtitleChoice::Off);
        lib.put_video(&uid, &video).unwrap();
        video.series.unwrap()
    };
    let (mut app, mut h) = desk.launch();
    h.state_until("home series", |s| {
        s["library_page"]["series"][0]["id"] == id
    });
    h.cmd(json!({"cmd":"library", "action":"analyse-series", "id":id}));
    let s = h.state_until("series counted without playing", |s| {
        score(s)["scored"] == 2
    });
    assert_eq!(score(&s)["total"], 4);
    assert_eq!(score(&s)["failed"], 0);
    assert!(s["library_page"]["open_series"].is_null());
    assert_eq!(s["loaded"], false);
    h.cmd(json!({"cmd":"library", "action":"series", "id":id}));
    let s = h.state_until("episode totals available", |s| {
        let e = &s["library_page"]["open_series"]["episodes"];
        e[0]["score"].is_object() && e[1]["score"].is_object()
    });
    for key in ["words", "known", "lines", "clear"] {
        let e = &s["library_page"]["open_series"]["episodes"];
        assert_eq!(
            score(&s)[key].as_u64().unwrap(),
            e[0]["score"][key].as_u64().unwrap() + e[1]["score"][key].as_u64().unwrap(),
            "{key}"
        );
    }
    let known = desk.path("known.csv");
    std::fs::write(&known, "word,reading,status\n食べる,たべる,known\n").unwrap();
    h.cmd(json!({"cmd":"words", "action":"import", "path":known}));
    h.state_until("retained series follows shared status", |s| {
        score(s)["known"] == 10
    });
    std::fs::write(&sidecar, "1\n00:00:01,000 --> 00:00:02,000\n日本語\n").unwrap();
    h.cmd(json!({"cmd":"library", "action":"analyse-series", "id":id}));
    let s = h.state_until("series refresh rereads changed text", |s| {
        score(s)["known"] == 1
    });
    assert_eq!(s["library_page"]["open_series"]["id"], id);
    assert_eq!(s["loaded"], false);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let lib = Library::open(&desk.data_file("library.redb")).unwrap();
    assert!(lib.segments().unwrap().is_empty());
    assert!(lib.word_encounters().unwrap().is_empty());
}
