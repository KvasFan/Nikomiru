// SPDX-License-Identifier: GPL-3.0-only

//! Reader statuses and chapter analysis through the real dictionary worker.

use super::support::{Desktop, exit_status};
use serde_json::{Value, json};
use std::time::Duration;

fn eat(s: &Value) -> Option<&Value> {
    s["books"]["words"]
        .as_array()?
        .iter()
        .find(|w| w["expression"] == "食べる")
}

fn epub(vertical: bool) {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!(
        "[subtitles]\nword_colors = true\n[books]\nvertical = {vertical}\n"
    ));
    let path = desk.path("book.epub");
    nikomiru_books::sample::write(&path).unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books", "action":"open"}));
    h.cmd(json!({"cmd":"books", "action":"add", "path":path.to_string_lossy()}));
    let first = h.state_until("first chapter counted and underlined", |s| {
        eat(s).is_some() && s["books"]["marks"]["unknown"].as_u64() > Some(0)
    });
    assert_eq!(first["books"]["vertical"], vertical);
    assert_eq!(eat(&first).unwrap()["count"], 1);
    h.cmd(json!({"cmd":"books", "action":"next"}));
    h.state_until("second chapter counted", |s| {
        s["books"]["chapter"] == 1
            && s["books"]["score"]["paragraphs"] == 1
            && eat(s).is_some_and(|w| w["status"] == "unknown")
    });
    h.cmd(json!({"cmd":"books", "action":"click", "paragraph":1, "offset":9}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"word_status", "level":0, "entry":0, "status":"learning"}));
    h.key("Escape");
    h.state_until("learning in the reader and list", |s| {
        s["books"]["marks"]["learning"].as_u64() > Some(0)
            && eat(s).is_some_and(|w| w["status"] == "learning")
    });
    h.cmd(json!({"cmd":"books", "action":"click", "paragraph":1, "offset":9}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"word_status", "level":0, "entry":0, "status":"known"}));
    h.key("Escape");
    h.state_until("known in the chapter score and list", |s| {
        s["books"]["marks"]["learning"] == 0
            && s["books"]["score"]["known"] == 1
            && eat(s).is_some_and(|w| w["status"] == "known")
    });
    h.cmd(json!({"cmd":"books", "action":"close"}));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    // A restart keeps word statuses and the chapter, but colouring off queues
    // no chapter analysis and draws no status marks.
    desk.write_config("[subtitles]\nword_colors = false\n");
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books", "action":"open"}));
    h.cmd(json!({"cmd":"books", "action":"add", "path":path.to_string_lossy()}));
    let s = h.state_until("the saved chapter without analysis", |s| {
        s["books"]["chapter"] == 1
    });
    assert!(s["books"]["score"].is_null() && s["books"]["words"].is_null());
    assert_eq!(s["books"]["marks"], json!({"unknown":0,"learning":0}));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-KNW-002, T-KNW-014, T-KNW-015
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn horizontal_epub_statuses_and_chapter_words() {
    epub(false);
}

// T-KNW-002, T-KNW-014, T-KNW-015
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn vertical_epub_statuses_and_chapter_words() {
    epub(true);
}

// T-KNW-002, T-KNW-014, T-KNW-015, T-E2E-WEB-001
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_web_page_counts_repeated_words_and_uses_shared_statuses() {
    let online = nikomiru_online::fake::FakeOnline::start().unwrap();
    online.data().pages.insert("words.html".into(),
        "<html><head><title>単語</title></head><body><main><p>食べました</p><p>食べました</p></main></body></html>".as_bytes().to_vec());
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors = true\n");
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books", "action":"open"}));
    h.cmd(json!({"cmd":"books", "action":"page", "path":online.page_url("words.html")}));
    let s = h.state_until("web words and marks", |s| {
        eat(s).is_some() && s["books"]["marks"]["unknown"].as_u64() > Some(0)
    });
    assert_eq!(eat(&s).unwrap()["count"], 2);
    assert_eq!(
        s["books"]["score"],
        json!({"words":2,"known":0,"paragraphs":2,"clear":0})
    );
    h.cmd(json!({"cmd":"books", "action":"click", "paragraph":0,"offset":0}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"word_status", "level":0,"entry":0,"status":"known"}));
    h.key("Escape");
    h.state_until("both occurrences known", |s| {
        s["books"]["score"]["known"] == 2
            && s["books"]["score"]["clear"] == 2
            && s["books"]["marks"]["unknown"] == 0
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
