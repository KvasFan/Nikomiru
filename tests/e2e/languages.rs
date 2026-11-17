// SPDX-License-Identifier: GPL-3.0-only

//! M115: Japanese and English studied side by side in the actual app.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::io::Write;
use std::time::Duration;

/// A small English Yomitan dictionary that declares its language.
fn english_dictionary(path: &std::path::Path) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("index.json", options).unwrap();
    zip.write_all(
        br#"{"title":"Mini EN","revision":"1","format":3,"sourceLanguage":"en","targetLanguage":"en"}"#,
    )
    .unwrap();
    zip.start_file("term_bank_1.json", options).unwrap();
    zip.write_all(
        br#"[["run","","v","v",0,["to move fast"],1,""],["cat","","n","n",0,["a small animal"],2,""],["fast","","adj","",0,["quick"],3,""]]"#,
    )
    .unwrap();
    zip.finish().unwrap();
}

fn import(desk: &Desktop, zip: &std::path::Path) {
    use std::sync::Arc;
    let data = desk.path("data").join("nikomiru");
    let library = Arc::new(nikomiru_library::Library::open(&data.join("library.redb")).unwrap());
    let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(nikomiru_core::SystemClock::new());
    let manager =
        nikomiru_library::DictionaryManager::open(library, data.join("dicts"), clock, "ja")
            .unwrap();
    manager
        .import(zip, None, &nikomiru_core::Cancel::new(), |_| {})
        .unwrap();
}

fn word_status(state: &serde_json::Value, expression: &str) -> Option<String> {
    state["books"]["words"]
        .as_array()?
        .iter()
        .find(|w| w["expression"] == expression)
        .map(|w| w["status"].as_str().unwrap_or_default().to_string())
}

// T-LNG-001…T-LNG-003: an English profile with its own dictionary, deinflection,
// whole words, statuses, counts and review card, isolated from Japanese, live
// and after restart.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn english_and_japanese_are_studied_apart() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let en = desk.path("mini-en.zip");
    english_dictionary(&en);
    import(&desk, &en);
    desk.write_config("[subtitles]\nword_colors = true\n[anki]\ndestination = \"nikomiru\"\n");
    let ja_book = desk.path("Neko.txt");
    std::fs::write(&ja_book, "猫が好きです。\n").unwrap();
    let en_book = desk.path("Cats.txt");
    std::fs::write(&en_book, "The cats were running fast.\n").unwrap();
    let (mut app, mut h) = desk.launch();
    assert_eq!(h.state()["study_language"], "ja");
    // Japanese: 猫 known
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":ja_book}));
    h.state_until("Japanese words", |s| word_status(s, "猫").is_some());
    h.cmd(json!({"cmd":"books","action":"click","paragraph":0,"offset":0}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"word_status","level":0,"entry":0,"status":"known"}));
    h.key("Escape");
    h.state_until("猫 known", |s| {
        word_status(s, "猫").as_deref() == Some("known")
    });
    h.cmd(json!({"cmd":"books","action":"close"}));
    // switch to English, whole words
    h.cmd(json!({"cmd":"settings","tab":"general","save":"[general]\nstudy_language = \"en\"\n[languages.en]\nsegmentation = \"words\"\nreadings = false\n"}));
    h.cmd(json!({"cmd":"settings","tab":"close"}));
    assert_eq!(h.state()["study_language"], "en");
    h.cmd(json!({"cmd":"books","action":"add","path":en_book}));
    let read = h.state_until("English words", |s| {
        word_status(s, "run").is_some() && word_status(s, "cat").is_some()
    });
    assert_eq!(word_status(&read, "猫"), None);
    assert_eq!(
        word_status(&read, "cat").as_deref(),
        Some("unknown"),
        "{}",
        read["books"]["words"]
    );
    // "The " is 4 bytes: "cats" deinflects to cat, looked up whole
    h.cmd(json!({"cmd":"books","action":"click","paragraph":0,"offset":4}));
    let rendered = h.wait("popup-rendered");
    assert_eq!(rendered["headwords"][0], "cat", "{rendered}");
    h.cmd(json!({"cmd":"word_status","level":0,"entry":0,"status":"known"}));
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    h.wait("card-created");
    h.key("Escape");
    let counted = h.state_until("cat known in English", |s| {
        word_status(s, "cat").as_deref() == Some("known")
            && s["books"]["score"]["known"]
                .as_u64()
                .is_some_and(|k| k >= 1)
    });
    assert_eq!(word_status(&counted, "run").as_deref(), Some("unknown"));
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.cmd(json!({"cmd":"library","action":"review"}));
    h.state_until("one English card", |s| {
        s["library_page"]["review"]["counts"]["total"] == 1
    });
    // back to Japanese: its own statuses, no English card
    h.cmd(json!({"cmd":"settings","tab":"general","save":"[general]\nstudy_language = \"ja\"\n"}));
    h.cmd(json!({"cmd":"settings","tab":"close"}));
    h.cmd(json!({"cmd":"library","action":"review"}));
    h.state_until("no Japanese card", |s| {
        s["library_page"]["review"]["counts"]["total"] == 0
    });
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":ja_book}));
    let ja = h.state_until("Japanese again", |s| word_status(s, "猫").is_some());
    assert_eq!(word_status(&ja, "猫").as_deref(), Some("known"));
    h.cmd(json!({"cmd":"settings","tab":"general","save":"[general]\nstudy_language = \"en\"\n"}));
    h.cmd(json!({"cmd":"settings","tab":"close"}));
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    // after restart: English, with its own word known and its card
    let (mut app, mut h) = desk.launch();
    assert_eq!(h.state()["study_language"], "en");
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":en_book}));
    let after = h.state_until("English kept", |s| word_status(s, "cat").is_some());
    assert_eq!(word_status(&after, "cat").as_deref(), Some("known"));
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.cmd(json!({"cmd":"library","action":"review"}));
    h.state_until("the English card kept", |s| {
        s["library_page"]["review"]["counts"]["total"] == 1
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
