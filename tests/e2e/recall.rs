// SPDX-License-Identifier: GPL-3.0-only

//! M92–M101: finite recall, portable progress and explicit local summaries in the real application.
use super::support::{Desktop, exit_status, fixture, until};
use serde_json::json;
use std::time::Duration;

// T-KNW-053, T-KNW-054, T-KNW-055, T-KNW-058, T-KNW-059, T-KNW-060, T-KNW-061.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn recall_progress_is_finite_portable_and_independent_of_media() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    let before = h.state();
    h.cmd(json!({"cmd":"recall","action":"start","words":[{"expression":"猫","reading":"ねこ","count":2},{"expression":"犬","reading":"いぬ"}]}));
    let first = h.state()["recall"].clone();
    assert_eq!(first["pending"], json!([0, 1]));
    assert_eq!(first["revealed"], false);
    h.cmd(json!({"cmd":"key","code":"Space","text_focus":true}));
    h.key("Digit2");
    assert_eq!(h.state()["recall"]["revealed"], false);
    assert_eq!(h.state()["recall"]["history"], json!([]));
    h.key("Space");
    assert_eq!(h.state()["recall"]["revealed"], true);
    h.key("Digit1");
    assert_eq!(h.state()["recall"]["current"], "犬");
    let saved = desk.path("recall.json");
    h.cmd(json!({"cmd":"recall","action":"save","path":saved}));
    let file = std::fs::read_to_string(&saved).unwrap();
    assert!(!file.contains("status") && !file.contains("path"));
    h.key("Space");
    h.key("Digit2");
    h.cmd(json!({"cmd":"recall","action":"load","path":saved}));
    assert_eq!(h.state()["recall"]["id"], first["id"]);
    assert_eq!(h.state()["recall"]["current"], "犬");
    assert_eq!(h.state()["recall"]["revealed"], false);
    let bad = desk.path("bad-recall.json");
    std::fs::write(&bad, r#"{"format":1,"order":[999]}"#).unwrap();
    let current = h.state()["recall"].clone();
    h.cmd(json!({"cmd":"recall","action":"load","path":bad}));
    assert_eq!(h.state()["recall"], current);
    h.cmd(json!({"cmd":"recall","action":"skip"}));
    h.key("Space");
    h.key("Digit2");
    let done = h.state();
    assert_eq!(done["recall"]["done"], true);
    assert_eq!(
        done["recall"]["summary"],
        json!({"good":1,"again":1,"skipped":1,"attempts":2})
    );
    assert_eq!(done["word_status_undo"], false);
    assert_eq!(done["loaded"], before["loaded"]);
    assert_eq!(done["paused"], before["paused"]);
    assert_eq!(done["time_pos"], before["time_pos"]);
    let results = desk.path("results.tsv");
    h.cmd(json!({"cmd":"recall","action":"results","path":results}));
    let report = std::fs::read_to_string(results).unwrap();
    assert!(report.contains("猫\tねこ\t2\t1\t1\t0\tgood\ttrue"));
    assert!(report.contains("犬\tいぬ\t0\t0\t0\t1\tskipped\ttrue"));
    h.cmd(json!({"cmd":"recall","action":"focused"}));
    let focused = h.state()["recall"].clone();
    assert_ne!(focused["id"], first["id"]);
    assert_eq!(focused["cards"].as_array().unwrap().len(), 1);
    assert_eq!(focused["current"], "猫");
    h.cmd(json!({"cmd":"recall","action":"load","path":saved}));
    h.cmd(json!({"cmd":"recall","action":"restart","order":1}));
    assert_eq!(h.state()["recall"]["pending"], json!([1, 0]));
    assert_eq!(h.state()["recall"]["history"], json!([]));
    h.cmd(json!({"cmd":"recall","action":"restart","order":2,"seed":42}));
    let order = h.state()["recall"]["order"].clone();
    h.cmd(json!({"cmd":"recall","action":"restart","order":2,"seed":42}));
    assert_eq!(h.state()["recall"]["order"], order);
    h.cmd(json!({"cmd":"settings","tab":"general"}));
    h.key("Digit2");
    assert_eq!(h.state()["recall"]["history"], json!([]));
    assert_eq!(h.state()["recall"]["covered"], true);
    h.cmd(json!({"cmd":"settings","tab":"close"}));
    h.key("Escape");
    assert!(h.state()["recall"].is_null());
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-KNW-056, T-KNW-057: exact inflected sentence hints and foreground lookup retain paused playback.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn recall_hints_and_dictionary_lookup_preserve_playback_and_progress() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors = true\n");
    let video = desk.path("recall.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    std::fs::write(desk.path("recall.ja.srt"), "1\n00:00:01,000 --> 00:00:02,000\n食べました\n\n2\n00:00:03,000 --> 00:00:04,000\n食べる\n\n3\n00:00:05,000 --> 00:00:06,000\n猫\n").unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.state_until("complete vocabulary", |s| {
        s["subtitles"]["words"]
            .as_array()
            .is_some_and(|a| a.iter().any(|w| w["expression"] == "食べる"))
    });
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.state_until("paused", |s| s["paused"] == true);
    h.cmd(json!({"cmd":"recall","action":"start","text":"食べる"}));
    h.cmd(json!({"cmd":"recall","action":"lookup"}));
    assert!(h.state()["popups"].as_array().unwrap().is_empty());
    h.cmd(json!({"cmd":"recall","action":"hints"}));
    assert_eq!(
        h.state()["recall"]["cards"][0]["hints"],
        json!(["食べました", "食べる"])
    );
    h.cmd(json!({"cmd":"recall","action":"hint"}));
    assert_eq!(h.state()["recall"]["hint_revealed"], true);
    assert_eq!(h.state()["recall"]["revealed"], false);
    h.key("Space");
    h.cmd(json!({"cmd":"library","action":"open"}));
    h.cmd(json!({"cmd":"recall","action":"lookup"}));
    h.state_until("recall dictionary result", |s| {
        s["popups"][0]["query"] == "食べる"
            && s["popups"][0]["entries"].as_u64().is_some_and(|n| n > 0)
    });
    h.key("Digit2"); // Dictionary context has priority over recall shortcuts.
    assert_eq!(h.state()["recall"]["history"], json!([]));
    h.key("Escape");
    assert_eq!(h.state()["recall"]["revealed"], true);
    assert!(h.state()["popups"].as_array().unwrap().is_empty());
    h.cmd(json!({"cmd":"recall","action":"lookup"}));
    h.state_until("lookup opened again", |s| {
        !s["popups"].as_array().unwrap().is_empty()
    });
    h.cmd(json!({"cmd":"recall","action":"again"}));
    let after = h.state();
    assert!(after["popups"].as_array().unwrap().is_empty());
    assert_eq!(after["recall"]["revealed"], false);
    assert_eq!(after["recall"]["hint_revealed"], false);
    assert_eq!(after["paused"], true);
    assert!(
        (after["time_pos"].as_f64().unwrap() - before["time_pos"].as_f64().unwrap()).abs() < 0.15
    );
    assert_eq!(after["word_status_undo"], false);
    h.key("Space");
    h.key("Digit2");
    h.cmd(json!({"cmd":"recall","action":"focused"}));
    assert_eq!(
        h.state()["recall"]["cards"][0]["hints"],
        json!(["食べました", "食べる"])
    );
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-KNW-062: explicit once-per-round aggregate persistence, failure/retry, process restart and removal.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn recall_summary_history_survives_restart_and_reports_write_failures() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"recall","action":"start","words":[{"expression":"秘密の単語","reading":"ひみつ"}]}));
    h.cmd(json!({"cmd":"recall","action":"record"}));
    assert_eq!(h.state()["recall_history"], json!([]));
    h.key("Space");
    h.key("Digit2");
    let id = h.state()["recall"]["id"].clone();
    let file = desk.path("complete.json");
    h.cmd(json!({"cmd":"recall","action":"save","path":file}));
    let config = desk.path("config/nikomiru/config.toml");
    if config.is_file() {
        std::fs::remove_file(&config).unwrap();
    }
    std::fs::create_dir_all(&config).unwrap();
    h.cmd(json!({"cmd":"recall","action":"record"}));
    h.state_until("failed history write", |s| {
        s["recall_history_failed"] == true
    });
    assert_eq!(h.state()["recall_history"].as_array().unwrap().len(), 1);
    std::fs::remove_dir(&config).unwrap();
    h.cmd(json!({"cmd":"recall","action":"retry"}));
    until(Duration::from_secs(10), "summary saved", || {
        let text = std::fs::read_to_string(&config).ok()?;
        (text.contains(id.as_str().unwrap()) && !text.contains("秘密の単語")).then_some(())
    });
    h.cmd(json!({"cmd":"recall","action":"load","path":file}));
    h.cmd(json!({"cmd":"recall","action":"record"}));
    assert_eq!(h.state()["recall_history"].as_array().unwrap().len(), 1);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    assert!(h.state()["recall"].is_null());
    assert_eq!(h.state()["recall_history"][0]["id"], id);
    assert_eq!(h.state()["recall_history"][0]["attempts"], 1);
    h.cmd(json!({"cmd":"recall","action":"history"}));
    assert_eq!(h.state()["recall_history_open"], true);
    h.key("Escape");
    assert_eq!(h.state()["recall_history_open"], false);
    h.cmd(json!({"cmd":"recall","action":"delete","id":id}));
    until(Duration::from_secs(10), "summary removed", || {
        let text = std::fs::read_to_string(&config).ok()?;
        (!text.contains(id.as_str().unwrap())).then_some(())
    });
    h.cmd(json!({"cmd":"recall","action":"load","path":file}));
    h.cmd(json!({"cmd":"recall","action":"restart"}));
    h.key("Space");
    h.key("Digit2");
    h.cmd(json!({"cmd":"recall","action":"record"}));
    assert_eq!(h.state()["recall_history"].as_array().unwrap().len(), 1);
    h.cmd(json!({"cmd":"recall","action":"delete"}));
    assert_eq!(h.state()["recall_history"], json!([]));
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    assert_eq!(h.state()["recall_history"], json!([]));
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
