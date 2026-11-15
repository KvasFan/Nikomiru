// SPDX-License-Identifier: GPL-3.0-only

//! M102: real file/paste import, lookup/mining, source identity and reading progress.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::{path::PathBuf, time::Duration};

// T-BOK-011, T-BOK-012, T-BOK-014: Markdown shares actual lookup/mining and analysis and keeps its place when moved.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn loose_markdown_uses_the_reader_and_keeps_progress_when_moved() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors=true\n[anki]\ndestination=\"nikomiru\"\n");
    let path = desk.path("Study.md");
    let mut source = String::from("# 読書\n\n食べました\n\n<ruby>猫<rt>ねこ</rt></ruby>\n\n");
    for n in 0..60 {
        source.push_str(&format!("段落{n}。食べる。\n\n"));
    }
    std::fs::write(&path, &source).unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    let before = h.state();
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    let first = h.state_until("Markdown reader and vocabulary", |s| {
        s["books"]["open"] == "Study"
            && s["books"]["words"]
                .as_array()
                .is_some_and(|w| w.iter().any(|w| w["expression"] == "食べる"))
    });
    let uid = first["books"]["uid"].clone();
    assert!(uid.as_str().unwrap().starts_with("text:"));
    assert_eq!(first["books"]["paragraphs"][0]["heading"], true);
    assert_eq!(first["books"]["paragraphs"][2]["ruby"][0]["text"], "ねこ");
    h.cmd(json!({"cmd":"books","action":"click","paragraph":1,"offset":0}));
    h.wait("popup-rendered");
    assert_eq!(h.state()["popups"][0]["query"], "食べました");
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    h.wait("card-created");
    h.key("Escape");
    h.cmd(json!({"cmd":"pointer","x":500.0,"y":400.0}));
    h.cmd(json!({"cmd":"wheel","y":1600.0}));
    h.state_until("reader moved beyond first page", |s| {
        s["books"]["paragraph"].as_u64().is_some_and(|p| p >= 15)
    });
    h.cmd(json!({"cmd":"books","action":"close"}));
    assert!(
        h.state()["books"]["list"][0]["read"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "saved position: {}",
        h.state()["books"]
    );
    let moved = desk.path("moved/Study.md");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&path, &moved).unwrap();
    h.cmd(json!({"cmd":"books","action":"add","path":moved}));
    h.state_until("moved document keeps its place", |s| {
        s["books"]["uid"] == uid && s["books"]["paragraph"].as_u64().is_some_and(|p| p >= 15)
    });
    let invalid = desk.path("invalid.txt");
    std::fs::write(&invalid, [0xff]).unwrap();
    h.cmd(json!({"cmd":"books","action":"add","path":invalid}));
    h.state_until("invalid text reported", |s| {
        s["books"]["status"]
            .as_str()
            .is_some_and(|t| t.contains("UTF-8"))
    });
    assert_eq!(h.state()["books"]["uid"], uid);
    assert_eq!(h.state()["paused"], before["paused"]);
    assert_eq!(h.state()["loaded"], before["loaded"]);
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    h.state_until("whole document analysis", |s| {
        s["books"]["list"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["uid"] == uid && b["whole_score"]["words"].as_u64().is_some_and(|n| n > 0))
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":uid}));
    h.state_until("reading position survives restart", |s| {
        s["books"]["uid"] == uid && s["books"]["paragraph"].as_u64().is_some_and(|p| p >= 15)
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-BOK-011, T-BOK-013, T-BOK-014: managed paste files are durable, isolate equal-size contents and fail without replacing the reader.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn pasted_text_reopens_and_failed_saving_preserves_the_active_book() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books-paste","title":"Notes","text":"猫"}));
    let first = h.state_until("pasted reader", |s| s["books"]["open"] == "Notes");
    let first_id = first["books"]["uid"].clone();
    let source = PathBuf::from(first["books"]["path"].as_str().unwrap());
    assert!(source.is_file());
    let root = source.parent().unwrap();
    let retained = root.with_extension("retained");
    std::fs::rename(root, &retained).unwrap();
    std::fs::write(root, "blocked").unwrap();
    h.cmd(json!({"cmd":"books-paste","title":"Notes","text":"犬"}));
    h.state_until("save failure", |s| {
        s["books"]["status"].as_str().is_some_and(|s| !s.is_empty())
    });
    assert_eq!(h.state()["books"]["uid"], first_id);
    std::fs::remove_file(root).unwrap();
    std::fs::rename(&retained, root).unwrap();
    h.cmd(json!({"cmd":"books-paste","title":"Notes","text":"犬"}));
    h.state_until("equal-length text has a different identity", |s| {
        s["books"]["uid"].is_string() && s["books"]["uid"] != first_id
    });
    assert_eq!(h.state()["books"]["list"].as_array().unwrap().len(), 2);
    h.cmd(json!({"cmd":"books-paste","title":"Notes","text":"猫"}));
    h.state_until("same paste reuses identity", |s| {
        s["books"]["uid"] == first_id
    });
    assert_eq!(h.state()["books"]["list"].as_array().unwrap().len(), 2);
    let literal = desk.path("Literal.txt");
    std::fs::write(&literal, "\u{feff}<b>猫&amp;</b>\r\n犬").unwrap();
    h.cmd(json!({"cmd":"books","action":"add","path":literal}));
    let plain = h.state_until("plain text remains literal", |s| {
        s["books"]["open"] == "Literal"
    });
    assert_eq!(plain["books"]["paragraphs"][0]["text"], "<b>猫&amp;</b>");
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":first_id}));
    let reopened = h.state_until("saved paste reopens", |s| s["books"]["uid"] == first_id);
    assert_eq!(reopened["books"]["paragraphs"][0]["text"], "猫");
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
