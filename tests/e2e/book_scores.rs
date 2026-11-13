// SPDX-License-Identifier: GPL-3.0-only

//! KNW-016: whole-book scores from the list, without opening the reader.

use super::support::{Desktop, exit_status};
use nikomiru_library::{
    Library,
    books::{BookRecord, book_uid},
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

fn seed(desk: &Desktop, path: &Path) -> String {
    let book = nikomiru_books::read(path).unwrap();
    let chars = book.chars();
    let uid = book_uid(&book.title, &book.author, chars);
    Library::open(&desk.data_file("library.redb"))
        .unwrap()
        .put_book(
            &uid,
            &BookRecord {
                path: path.to_string_lossy().into(),
                title: book.title,
                author: book.author,
                chars,
                read_chars: 1,
                chapter: 0,
                paragraph: 0,
                last_read_at: Some(42),
            },
        )
        .unwrap();
    uid
}

fn score(s: &Value) -> &Value {
    &s["books"]["list"][0]["whole_score"]
}

// T-KNW-016: every chapter, shared statuses, saved place and cached result.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn an_epub_is_scored_before_opening_and_keeps_its_reading_place() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors = true\n");
    let path = desk.path("book.epub");
    nikomiru_books::sample::write(&path).unwrap();
    let uid = seed(&desk, &path);
    {
        let lib = Library::open(&desk.data_file("library.redb")).unwrap();
        let mut record = lib.book(&uid).unwrap().unwrap();
        record.chapter = 1;
        record.paragraph = 1;
        lib.put_book(&uid, &record).unwrap();
    }
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.state_until("the saved book", |s| s["books"]["list"][0]["uid"] == uid);
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    let s = h.state_until("whole EPUB counted", |s| score(s)["words"].is_u64());
    assert!(s["books"]["open"].is_null());
    assert_eq!(s["books"]["list"][0]["last_read_at"], 42);
    assert_eq!(s["books"]["list"][0]["chapter"], 1);
    assert_eq!(s["books"]["list"][0]["read"], 1);
    let known = desk.path("known.csv");
    std::fs::write(&known, "word,reading,status\n食べる,たべる,known\n").unwrap();
    h.cmd(json!({"cmd":"words","action":"import","path":known}));
    h.state_until("both chapters rescored", |s| score(s)["known"] == 2);
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    let s = h.state_until("the saved chapter open", |s| {
        s["books"]["chapter"] == 1 && s["books"]["score"]["known"] == 1
    });
    assert_eq!(
        score(&s)["known"],
        2,
        "whole book and current chapter are distinct"
    );
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.state_until("the retained whole score", |s| {
        s["books"]["open"].is_null() && score(s)["known"] == 2
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-KNW-016: explicit refresh, no partial scores, no network or study writes.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_saved_web_book_refreshes_and_limits_never_leave_an_old_score() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors = true\n");
    let path = desk.path("page.html");
    std::fs::write(&path, "<title>Page</title><main><p>食べる</p></main>").unwrap();
    let uid = seed(&desk, &path);
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    h.state_until("one unknown word", |s| {
        score(s)["words"] == 1 && score(s)["known"] == 0
    });
    let known = desk.path("known.csv");
    std::fs::write(&known, "word,reading,status\n食べる,たべる,known\n").unwrap();
    h.cmd(json!({"cmd":"words","action":"import","path":known}));
    h.state_until("known in the retained count", |s| score(s)["known"] == 1);
    // Same title/author/character count keeps the library uid.
    std::fs::write(&path, "<title>Page</title><main><p>日本語</p></main>").unwrap();
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    h.state_until("refresh counted the changed text", |s| {
        score(s)["words"] == 1 && score(s)["known"] == 0
    });
    std::fs::write(
        &path,
        format!(
            "<title>Page</title><main><p>{}</p></main>",
            "a".repeat(nikomiru_books::analysis::MAX_PARAGRAPH_BYTES + 1)
        ),
    )
    .unwrap();
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    h.state_until("whole analysis limit, no old score", |s| {
        score(s) == "too-large"
    });
    std::fs::remove_file(&path).unwrap();
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    let s = h.state_until("missing source, no old score", |s| {
        score(s)["error"].is_string()
    });
    assert!(s["books"]["open"].is_null());
    assert_eq!(s["books"]["list"][0]["last_read_at"], 42);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    {
        let lib = Library::open(&desk.data_file("library.redb")).unwrap();
        assert!(lib.segments().unwrap().is_empty());
        assert!(lib.word_encounters().unwrap().is_empty());
        assert_eq!(lib.book(&uid).unwrap().unwrap().last_read_at, Some(42));
    }
    desk.write_config("[subtitles]\nword_colors = false\n");
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    let s = h.state();
    assert!(score(&s).is_null());
    assert!(s["books"]["open"].is_null());
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
