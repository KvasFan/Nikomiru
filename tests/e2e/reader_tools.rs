// SPDX-License-Identifier: GPL-3.0-only

//! M111: annotations, library text search and reading modes in the actual app.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::time::Duration;

// T-RDR-001, T-RDR-002: explicit word/sentence annotations, bookmark names, TSV,
// validated reopening, restart and a replaced source never shown at a shifted place.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn annotations_are_explicit_persist_and_refuse_changed_sources() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let path = desk.path("Notes.txt");
    let mut text = String::from("始まりの文です。\n\n");
    for n in 0..30 {
        text.push_str(&format!("段落{n}。猫が好きです。\n\n"));
    }
    std::fs::write(&path, &text).unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    let opened = h.state_until("reader", |s| s["books"]["open"] == "Notes");
    let uid = opened["books"]["uid"].clone();
    let statuses = opened["word_status_undo"].clone();
    // a word of paragraph 5: "段落4。" is 10 bytes, then 猫
    h.cmd(json!({"cmd":"books","action":"click","paragraph":5,"offset":10}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"reader-tools","action":"new","kind":"highlight"}));
    let draft = h.state()["reader_tools"]["editor"].clone();
    assert_eq!(draft["paragraph"], 5);
    assert_eq!(draft["start"], 10);
    assert!(draft["quote"].as_str().unwrap().starts_with('猫'));
    assert!(
        h.state()["reader_tools"]["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "nothing saved before Save"
    );
    h.cmd(json!({"cmd":"reader-tools","action":"save"}));
    h.state_until("highlight saved, back to the reader", |s| {
        s["reader_tools"]["busy"] == false
            && s["reader_tools"]["open"] == false
            && s["reader_tools"]["items"]
                .as_array()
                .is_some_and(|i| i.len() == 1)
    });
    // a note over the sentence, and a named bookmark
    h.cmd(json!({"cmd":"books","action":"click","paragraph":7,"offset":10}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"reader-tools","action":"new","kind":"note"}));
    h.cmd(json!({"cmd":"reader-tools","action":"scope","scope":"sentence"}));
    h.cmd(json!({"cmd":"reader-tools","action":"edit","note":"=cats\nare nice"}));
    assert_eq!(
        h.state()["reader_tools"]["editor"]["quote"],
        "猫が好きです。"
    );
    h.cmd(json!({"cmd":"reader-tools","action":"save"}));
    h.state_until("note saved", |s| {
        s["reader_tools"]["busy"] == false
            && s["reader_tools"]["items"]
                .as_array()
                .is_some_and(|i| i.len() == 2)
    });
    h.cmd(json!({"cmd":"reader-tools","action":"new","kind":"bookmark"}));
    h.cmd(json!({"cmd":"reader-tools","action":"edit","name":""}));
    h.cmd(json!({"cmd":"reader-tools","action":"save"}));
    let refused = h.state_until("unnamed bookmark refused", |s| {
        s["reader_tools"]["busy"] == false && s["reader_tools"]["error"].is_string()
    });
    assert!(refused["reader_tools"]["editor"].is_object(), "draft kept");
    h.cmd(json!({"cmd":"reader-tools","action":"edit","name":"Start"}));
    h.cmd(json!({"cmd":"reader-tools","action":"save"}));
    let saved = h.state_until("bookmark saved", |s| {
        s["reader_tools"]["busy"] == false
            && s["reader_tools"]["items"]
                .as_array()
                .is_some_and(|i| i.len() == 3)
    });
    assert_eq!(
        saved["word_status_undo"], statuses,
        "no word status changed"
    );
    let items = saved["reader_tools"]["items"].as_array().unwrap().clone();
    assert!(items.iter().all(|a| a["current"] == true));
    let note = items.iter().find(|a| a["kind"] == "note").unwrap().clone();
    h.cmd(json!({"cmd":"reader-tools","action":"close"}));
    let drawn = h.state()["books"]["annotated"].clone();
    assert!(
        drawn.as_array().unwrap().len() == 2,
        "highlight and note drawn: {drawn}"
    );
    let out = desk.path("annotations.tsv");
    h.cmd(json!({"cmd":"reader-tools","action":"export","query":"","path":out}));
    let tsv = std::fs::read_to_string(&out).unwrap();
    assert!(
        tsv.contains("note\t\t猫が好きです。\t'=cats are nice\tNotes"),
        "{tsv}"
    );
    assert!(tsv.contains("bookmark\tStart\t"));
    // closed, then reopened at the note through its checked text
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.cmd(json!({"cmd":"reader-tools","action":"go","id":note["id"]}));
    h.state_until("note reopened", |s| {
        s["books"]["uid"] == uid
            && s["books"]["paragraph"] == 7
            && s["books"]["found"] == json!([7, note["start"], note["end"]])
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    // restart keeps them; a replaced source is never opened at a shifted place
    std::fs::write(&path, text.replace("段落6。", "段落六。")).unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"reader-tools","action":"open"}));
    let state = h.state_until("annotations after restart", |s| {
        s["reader_tools"]["busy"] == false
            && s["reader_tools"]["items"]
                .as_array()
                .is_some_and(|i| i.len() == 3)
    });
    let kept = state["reader_tools"]["items"].as_array().unwrap();
    assert!(
        kept.iter()
            .any(|a| a["id"] == note["id"] && a["note"] == note["note"])
    );
    h.cmd(json!({"cmd":"reader-tools","action":"go","id":note["id"]}));
    let changed = h.state_until("changed source refused", |s| {
        s["books"]["busy"] == false && s["books"]["status"].is_string()
    });
    assert!(changed["books"]["found"].is_null());
    assert_ne!(changed["books"]["uid"], uid);
    h.cmd(json!({"cmd":"reader-tools","action":"open"}));
    // the list read on opening ends first: a request while busy is ignored,
    // as the disabled buttons are
    h.state_until("annotations listed again", |s| {
        s["reader_tools"]["open"] == true && s["reader_tools"]["busy"] == false
    });
    h.cmd(json!({"cmd":"reader-tools","action":"delete","id":note["id"]}));
    h.state_until("explicit deletion", |s| {
        s["reader_tools"]["busy"] == false
            && s["reader_tools"]["items"]
                .as_array()
                .is_some_and(|i| i.len() == 2)
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-RDR-003, T-RDR-004: cross-book search opens a checked result; pages and sentence
// focus turn explicitly, keep lookup offsets and the mode survives restart.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn library_search_and_reading_modes_follow_the_original_text() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let first = desk.path("First.txt");
    let second = desk.path("Second.md");
    let mut long = String::new();
    for n in 0..80 {
        long.push_str(&format!("文{n}です。もう一つの文。\n\n"));
    }
    std::fs::write(&first, format!("犬です。\n\n{long}")).unwrap();
    std::fs::write(&second, format!("# 二\n\n{long}最後に猫が来た。\n")).unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    for path in [&first, &second] {
        h.cmd(json!({"cmd":"books","action":"add","path":path}));
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        h.state_until("book read", |s| s["books"]["open"] == name.as_str());
    }
    h.cmd(json!({"cmd":"books","action":"close"}));
    let before = h.state();
    h.cmd(json!({"cmd":"reader-tools","action":"open","tab":"search"}));
    h.cmd(json!({"cmd":"reader-tools","action":"search","query":"\u{0}"}));
    assert!(h.state()["reader_tools"]["search"]["error"].is_string());
    h.cmd(json!({"cmd":"reader-tools","action":"search","query":"猫"}));
    h.cmd(json!({"cmd":"reader-tools","action":"search-cancel"}));
    let cancelled = h.state();
    assert_eq!(cancelled["reader_tools"]["search"]["running"], false);
    h.cmd(json!({"cmd":"reader-tools","action":"search","query":"猫"}));
    let found = h.state_until("search finished", |s| {
        s["reader_tools"]["search"]["running"] == false
            && s["reader_tools"]["search"]["outcome"].is_object()
    });
    let outcome = &found["reader_tools"]["search"]["outcome"];
    assert_eq!(outcome["searched"], 2);
    let hits = outcome["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "{outcome}");
    assert_eq!(hits[0]["title"], "Second");
    assert_eq!(hits[0]["start"], 9, "after 最後に");
    assert_eq!(
        found["books"]["list"], before["books"]["list"],
        "searching moves no reading place"
    );
    h.cmd(json!({"cmd":"reader-tools","action":"search","query":"もう一つ"}));
    let many = h.state_until("bounded results", |s| {
        s["reader_tools"]["search"]["running"] == false
            && s["reader_tools"]["search"]["query"] == "もう一つ"
            && s["reader_tools"]["search"]["outcome"].is_object()
    });
    assert_eq!(
        many["reader_tools"]["search"]["outcome"]["hits"]
            .as_array()
            .unwrap()
            .len(),
        40,
        "20 per book"
    );
    h.cmd(json!({"cmd":"reader-tools","action":"search","query":"猫"}));
    h.state_until("search again", |s| {
        s["reader_tools"]["search"]["running"] == false
            && s["reader_tools"]["search"]["query"] == "猫"
    });
    h.cmd(json!({"cmd":"reader-tools","action":"search-go","index":0}));
    let opened = h.state_until("result opened", |s| {
        s["books"]["open"] == "Second" && s["books"]["found"].is_array()
    });
    let paragraph = opened["books"]["found"][0].as_u64().unwrap();
    assert_eq!(paragraph, 81);
    // the view scrolls there (animated): the place reached follows
    h.state_until("scrolled to the result", |s| {
        s["books"]["paragraph"].as_u64().is_some_and(|p| p >= 60)
    });
    // pages
    h.cmd(json!({"cmd":"books","action":"mode","path":"pages"}));
    h.cmd(json!({"cmd":"books","action":"turn","paragraph":0}));
    let paged = h.state_until("first page", |s| {
        s["books"]["mode"] == "pages" && s["books"]["paragraph"] == 0
    });
    assert!(paged["reader_tools"]["open"] == false);
    h.key("PageDown");
    h.state_until("page turned by key", |s| {
        s["books"]["paragraph"].as_u64().is_some_and(|p| p > 0)
    });
    // sentence focus: next sentence crosses the paragraph, a click keeps offsets
    h.cmd(json!({"cmd":"books","action":"mode","path":"sentence"}));
    h.cmd(json!({"cmd":"books","action":"turn","paragraph":1}));
    h.cmd(json!({"cmd":"books","action":"next-sentence"}));
    h.state_until("second sentence", |s| {
        s["books"]["paragraph"] == 1 && s["books"]["sentence"] == 1
    });
    h.cmd(json!({"cmd":"books","action":"next-sentence"}));
    h.state_until("next paragraph", |s| {
        s["books"]["paragraph"] == 2 && s["books"]["sentence"] == 0
    });
    h.key("PageUp");
    h.state_until("back again", |s| {
        s["books"]["paragraph"] == 1 && s["books"]["sentence"] == 1
    });
    // "文0です。" is 13 bytes: もう一つ starts the second sentence
    h.cmd(json!({"cmd":"books","action":"click","paragraph":1,"offset":13}));
    h.wait("popup-rendered");
    assert_eq!(
        h.state()["popups"][0]["query"]
            .as_str()
            .unwrap()
            .chars()
            .next(),
        Some('も')
    );
    h.key("Escape");
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":opened["books"]["uid"]}));
    h.state_until("mode kept", |s| {
        s["books"]["open"] == "Second" && s["books"]["mode"] == "sentence"
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
