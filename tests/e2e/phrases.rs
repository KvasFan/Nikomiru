// SPDX-License-Identifier: GPL-3.0-only

//! M104: phrases follow the real notebook, serial writer, reader and native export handlers.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::time::Duration;

// T-PHR-001, T-PHR-002, T-PHR-003: frozen reader source, explicit commit, duplicate/error draft retention and restart isolation.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn phrase_notebook_captures_exports_reopens_and_persists_independent_states() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let path = desk.path("Phrases.txt");
    std::fs::write(&path, "猫が好きです。\n\n犬が好きです。\n").unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    h.state_until("reader", |s| s["books"]["open"] == "Phrases");
    h.cmd(json!({"cmd":"books","action":"click","paragraph":0,"offset":0}));
    h.wait("popup-rendered");
    h.key("Escape");
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    h.state_until("notebook loaded", |s| s["phrases"]["busy"] == false);
    h.cmd(json!({"cmd":"phrases","action":"capture"}));
    let draft = h.state()["phrases"]["editor"].clone();
    assert_eq!(draft["context"], "猫が好きです。");
    assert_eq!(draft["source"]["paragraph"], 0);
    assert!(h.state()["phrases"]["items"].as_array().unwrap().is_empty());
    h.cmd(json!({"cmd":"phrases","action":"edit","text":"猫が好き","definition":"=likes cats","status":"known"}));
    h.cmd(json!({"cmd":"phrases","action":"save"}));
    let saved = h.state_until("phrase committed", |s| {
        s["phrases"]["items"]
            .as_array()
            .is_some_and(|p| p.len() == 1)
            && s["phrases"]["busy"] == false
    });
    let row = saved["phrases"]["items"][0].clone();
    assert_eq!(row["status"], "Known");
    assert_eq!(saved["word_status_undo"], false);
    let out = desk.path("phrases.tsv");
    h.cmd(json!({"cmd":"phrases","action":"export","query":"猫が好き","path":out}));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("猫が好き\t'=likes cats\t猫が好きです。\tknown\tja"));
    // A second identical source/phrase is refused without throwing the draft away.
    h.cmd(json!({"cmd":"phrases","action":"capture"}));
    h.cmd(json!({"cmd":"phrases","action":"edit","text":"猫が好き"}));
    h.cmd(json!({"cmd":"phrases","action":"save"}));
    let failed = h.state_until("duplicate refused", |s| {
        s["phrases"]["error"].is_string() && s["phrases"]["busy"] == false
    });
    assert!(failed["phrases"]["editor"].is_object());
    assert_eq!(failed["phrases"]["items"][0], row);
    h.cmd(json!({"cmd":"phrases","action":"cancel"}));
    h.key("Escape");
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    h.state_until("notebook refreshed", |s| s["phrases"]["busy"] == false);
    h.cmd(json!({"cmd":"phrases","action":"source","id":row["id"]}));
    h.state_until("source reopened", |s| {
        s["books"]["uid"] == row["source"]["uid"] && s["books"]["chapter"] == 0
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    let state = h.state_until("phrase survives restart", |s| {
        s["phrases"]["busy"] == false
            && s["phrases"]["items"]
                .as_array()
                .is_some_and(|p| p.len() == 1)
    });
    assert_eq!(state["phrases"]["items"][0], row);
    assert_eq!(state["word_status_undo"], false);
    h.cmd(json!({"cmd":"phrases","action":"delete","id":row["id"]}));
    h.state_until("explicit deletion", |s| {
        s["phrases"]["busy"] == false && s["phrases"]["items"].as_array().is_some_and(Vec::is_empty)
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-PHR-003: primary captions keep their interval/context, capture/save never seek or resume video.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn caption_phrase_capture_keeps_playback_and_opens_its_explicit_scene() {
    let mut desk = Desktop::start();
    let subtitles = desk.path("phrase.ja.srt");
    std::fs::write(
        &subtitles,
        "1\n00:00:00,000 --> 00:01:00,000\n猫が好きです。\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&super::support::fixture("av_long.mkv"));
    h.wait("first-frame");
    h.drop_files(&[subtitles.to_str().unwrap()]);
    h.state_until("one primary caption", |s| s["subtitles"]["cues"] == 1);
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.paused_settled("paused source");
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    h.state_until("notebook", |s| s["phrases"]["busy"] == false);
    h.cmd(json!({"cmd":"phrases","action":"capture"}));
    let draft = h.state()["phrases"]["editor"].clone();
    assert_eq!(draft["context"], "猫が好きです。");
    assert_eq!(draft["source"]["book"], false);
    assert_eq!(draft["source"]["start_ms"], 0);
    assert_eq!(draft["source"]["end_ms"], 60000);
    h.cmd(json!({"cmd":"phrases","action":"edit","text":"猫が好き"}));
    h.cmd(json!({"cmd":"phrases","action":"save"}));
    let saved = h.state_until("caption phrase saved", |s| {
        s["phrases"]["busy"] == false
            && s["phrases"]["items"]
                .as_array()
                .is_some_and(|p| p.len() == 1)
    });
    assert_eq!(saved["time_pos"], before["time_pos"]);
    assert_eq!(saved["paused"], true);
    assert_eq!(saved["word_status_undo"], false);
    h.cmd(json!({"cmd":"phrases","action":"source","id":saved["phrases"]["items"][0]["id"]}));
    h.state_until("scene request paused", |s| {
        s["phrases"]["open"] == false
            && s["paused"] == true
            && s["time_pos"].as_f64().is_some_and(|t| t < 0.2)
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
