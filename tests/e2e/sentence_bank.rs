// SPDX-License-Identifier: GPL-3.0-only

//! M106: real dictionary identities across reader/subtitle sources, frozen original positions and owned packs.
use super::support::{Desktop, Harness, exit_status, fixture};
use serde_json::{Value, json};
use std::time::Duration;
fn query(h: &mut Harness, expression: &str, reading: &str) -> Value {
    h.cmd(
        json!({"cmd":"sentence-bank","action":"query","expression":expression,"reading":reading}),
    );
    h.state_until("exact sentence query", |s| {
        s["sentence_bank"]["querying"] == false
            && s["sentence_bank"]["expression"] == expression
            && s["sentence_bank"]["reading"] == reading
    })
}
fn index(h: &mut Harness, kind: &str, count: usize) {
    h.cmd(json!({"cmd":"sentence-bank","action":kind}));
    h.state_until("complete source batch", |s| {
        s["sentence_bank"]["indexing"] == false
            && s["sentence_bank"]["batches"]
                .as_array()
                .is_some_and(|b| b.len() == count)
    });
}
// T-SEN-001, T-SEN-002, T-SEN-003: actual segmenter, multiple books, Japanese byte locations, replaced/missing files, export/deletion/restart.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn sentence_bank_uses_exact_inflected_book_examples_and_verified_original_sources() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let first = desk.path("First.txt");
    let second = desk.path("Second.txt");
    std::fs::write(&first, "前文です。食べました。猫。\n\n二段落です。食べる。").unwrap();
    std::fs::write(&second, "別の文。食べる。犬。").unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":first}));
    let first_state = h.state_until("first source reader", |s| s["books"]["open"] == "First");
    let first_uid = first_state["books"]["uid"].clone();
    h.cmd(json!({"cmd":"sentence-bank","action":"open"}));
    index(&mut h, "book", 1);
    assert_eq!(
        h.state()["books"]["paragraph"],
        first_state["books"]["paragraph"]
    );
    h.cmd(json!({"cmd":"sentence-bank","action":"close"}));
    h.cmd(json!({"cmd":"books","action":"add","path":second}));
    let second_state = h.state_until("second source reader", |s| s["books"]["open"] == "Second");
    let second_uid = second_state["books"]["uid"].clone();
    h.cmd(json!({"cmd":"sentence-bank","action":"open"}));
    index(&mut h, "book", 2);
    let state = query(&mut h, "食べる", "たべる");
    let examples = state["sentence_bank"]["examples"].as_array().unwrap();
    assert_eq!(examples.len(), 3);
    let at = examples
        .iter()
        .position(|e| e["sentence"]["text"] == "食べました。")
        .unwrap();
    assert_eq!(examples[at]["sentence"]["source"]["uid"], first_uid);
    assert_eq!(examples[at]["sentence"]["source"]["offset"], 15);
    assert_eq!(examples[at]["ranges"], json!([[0, 15]]));
    assert_eq!(state["word_status_undo"], false);
    assert_eq!(state["books"]["uid"], second_uid);
    let empty = query(&mut h, "食べる", "wrong");
    assert!(
        empty["sentence_bank"]["examples"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let state = query(&mut h, "食べる", "たべる");
    let at = state["sentence_bank"]["examples"]
        .as_array()
        .unwrap()
        .iter()
        .position(|e| e["sentence"]["text"] == "食べました。")
        .unwrap();
    let out = desk.path("examples.tsv");
    h.cmd(json!({"cmd":"sentence-bank","action":"export","path":out}));
    let text = std::fs::read_to_string(out).unwrap();
    assert!(text.contains("食べました。\tja\tFirst"));
    assert!(text.contains("0:15"));
    h.cmd(json!({"cmd":"sentence-bank","action":"source","index":at}));
    h.state_until("original book location", |s| {
        s["books"]["uid"] == first_uid
            && s["books"]["paragraph"] == 0
            && s["books"]["busy"] == false
    });
    // The same path now contains different text: verification fails before replacing the active reader.
    h.cmd(json!({"cmd":"books","action":"add","path":second}));
    h.state_until("other reader kept for stale check", |s| {
        s["books"]["uid"] == second_uid
    });
    std::fs::write(&first, "前文です。飲みました。猫。\n\n二段落です。食べる。").unwrap();
    let state = query(&mut h, "食べる", "たべる");
    let at = state["sentence_bank"]["examples"]
        .as_array()
        .unwrap()
        .iter()
        .position(|e| e["sentence"]["text"] == "食べました。")
        .unwrap();
    h.cmd(json!({"cmd":"sentence-bank","action":"source","index":at}));
    h.state_until("changed source rejected", |s| {
        s["books"]["status"]
            .as_str()
            .is_some_and(|t| t.contains("changed"))
    });
    assert_eq!(h.state()["books"]["uid"], second_uid);
    std::fs::remove_file(&first).unwrap();
    let state = query(&mut h, "食べる", "たべる");
    let at = state["sentence_bank"]["examples"]
        .as_array()
        .unwrap()
        .iter()
        .position(|e| e["sentence"]["text"] == "食べました。")
        .unwrap();
    h.cmd(json!({"cmd":"sentence-bank","action":"source","index":at}));
    assert!(
        h.state()["sentence_bank"]["error"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );
    let key = state["sentence_bank"]["batches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["title"] == "First")
        .unwrap()["key"]
        .clone();
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    let state = query(&mut h, "食べる", "たべる");
    assert_eq!(
        state["sentence_bank"]["examples"].as_array().unwrap().len(),
        3
    );
    h.cmd(json!({"cmd":"sentence-bank","action":"delete","key":key}));
    h.state_until("whole source deleted", |s| {
        s["sentence_bank"]["batches"]
            .as_array()
            .is_some_and(|b| b.len() == 1)
    });
    let state = query(&mut h, "食べる", "たべる");
    assert_eq!(
        state["sentence_bank"]["examples"].as_array().unwrap().len(),
        1
    );
    assert_eq!(state["word_status_undo"], false);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
// T-SEN-001, T-SEN-002, T-SEN-003: primary timed examples and owned imports share offline queries; malformed packs cannot replace existing data.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn caption_and_owned_pack_examples_keep_playback_and_reject_invalid_imports_atomically() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let subtitles = desk.path("bank.ja.srt");
    std::fs::write(
        &subtitles,
        "1\n00:00:00,000 --> 00:00:30,000\n食べました。\n",
    )
    .unwrap();
    let pack = desk.path("owned.json");
    std::fs::write(&pack,serde_json::to_vec(&json!({"version":1,"title":"=Owned pack","entries":[{"text":"猫です。食べる。","language":"ja"}]})).unwrap()).unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("first-frame");
    h.drop_files(&[subtitles.to_str().unwrap()]);
    h.state_until("primary text", |s| s["subtitles"]["cues"] == 1);
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.state_until("source paused", |s| s["paused"] == true);
    h.cmd(json!({"cmd":"sentence-bank","action":"open"}));
    index(&mut h, "track", 1);
    h.cmd(json!({"cmd":"sentence-bank","action":"import","path":pack}));
    h.state_until("owned pack committed", |s| {
        s["sentence_bank"]["indexing"] == false
            && s["sentence_bank"]["batches"]
                .as_array()
                .is_some_and(|b| b.len() == 2)
    });
    let state = query(&mut h, "食べる", "たべる");
    assert_eq!(
        state["sentence_bank"]["examples"].as_array().unwrap().len(),
        2
    );
    assert_eq!(state["time_pos"], before["time_pos"]);
    assert_eq!(state["paused"], true);
    assert_eq!(state["word_status_undo"], false);
    let examples = state["sentence_bank"]["examples"].as_array().unwrap();
    let timed = examples
        .iter()
        .find(|e| e["sentence"]["source"]["book"] == false)
        .unwrap();
    assert_eq!(timed["sentence"]["source"]["start_ms"], 0);
    assert_eq!(timed["sentence"]["source"]["end_ms"], 30000);
    let out = desk.path("owned.tsv");
    h.cmd(json!({"cmd":"sentence-bank","action":"export","path":out}));
    assert!(
        std::fs::read_to_string(out)
            .unwrap()
            .contains("'=Owned pack")
    );
    let bad = desk.path("bad.json");
    std::fs::write(
        &bad,
        r#"{"version":999,"title":"Bad","entries":[{"text":"食べる","language":"ja"}]}"#,
    )
    .unwrap();
    h.cmd(json!({"cmd":"sentence-bank","action":"import","path":bad}));
    h.state_until("invalid pack reported", |s| {
        s["sentence_bank"]["indexing"] == false && s["sentence_bank"]["error"].is_string()
    });
    assert_eq!(
        h.state()["sentence_bank"]["batches"],
        state["sentence_bank"]["batches"]
    );
    assert_eq!(
        query(&mut h, "食べる", "たべる")["sentence_bank"]["examples"],
        state["sentence_bank"]["examples"]
    );
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
