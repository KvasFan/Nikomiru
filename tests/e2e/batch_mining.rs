// SPDX-License-Identifier: GPL-3.0-only

//! M107: explicit previews, durable one-at-a-time exports, fault recovery and original contexts.
use super::support::{Desktop, Harness, exit_status, fixture, until};
use nikomiru_anki::fake::{Behaviour, FakeAnki};
use serde_json::{Value, json};
use std::time::Duration;
fn setup(desk: &mut Desktop, anki: &FakeAnki) {
    desk.import_dictionaries(&["mini-ja"]);
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&anki.url(), "").unwrap()).unwrap();
    desk.write_config(&format!(
        "[anki]\nurl=\"{}\"\n[subtitles]\nword_colors=true\n",
        anki.url()
    ));
}
fn book(desk: &Desktop, h: &mut Harness) {
    let path = desk.path("BatchSource.txt");
    std::fs::write(&path, "猫です。\n\n犬です。\n\n猫が好きです。").unwrap();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    h.state_until("owned batch source", |s| {
        s["books"]["open"] == "BatchSource"
    });
}
fn capture(h: &mut Harness, paragraph: usize) {
    h.cmd(json!({"cmd":"batch-mining","action":"close"}));
    if !h.state()["popups"].as_array().unwrap().is_empty() {
        h.key("Escape");
    }
    h.cmd(json!({"cmd":"books","action":"click","paragraph":paragraph,"offset":0}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"batch-mining","action":"lookup"}));
}
fn preview(h: &mut Harness) {
    h.cmd(json!({"cmd":"batch-mining","action":"preview"}));
    h.state_until("whole batch text preview", |s| {
        s["batch_mining"]["stage"] == "Ready"
    });
}
fn add_count(anki: &FakeAnki) -> usize {
    anki.requests()
        .iter()
        .filter(|r| r["action"] == "addNote")
        .count()
}
fn journal(state: &Value) -> Value {
    state["batch_mining"]["journals"].as_array().unwrap()[0].clone()
}
// T-BMG-001, T-BMG-002, T-BMG-003, T-BMG-004: chosen lookup contexts, no pre-Start add, one-item barriers, certain retry and duplicate skips.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn previewed_book_batches_retry_certain_failures_and_skip_existing_words() {
    let anki = FakeAnki::start().unwrap();
    let mut desk = Desktop::start();
    setup(&mut desk, &anki);
    let (mut app, mut h) = desk.launch();
    book(&desk, &mut h);
    capture(&mut h, 0);
    capture(&mut h, 1);
    capture(&mut h, 2);
    let state = h.state();
    assert_eq!(
        state["batch_mining"]["entries"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        state["batch_mining"]["entries"][0]["contexts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    h.cmd(json!({"cmd":"batch-mining","action":"edit","index":0,"context":1}));
    preview(&mut h);
    assert_eq!(add_count(&anki), 0);
    assert!(anki.collection().notes.is_empty());
    assert!(
        !anki
            .requests()
            .iter()
            .any(|r| r["action"] == "storeMediaFile")
    );
    let original = h.state()["batch_mining"]["entries"][0]["operation"].clone();
    anki.on(
        "addNote",
        Behaviour::Json(json!({"result":null,"error":"temporary test refusal"})),
    );
    h.cmd(json!({"cmd":"batch-mining","action":"start"}));
    let failed = h.state_until("certain failure recorded", |s| {
        s["batch_mining"]["stage"] == "Stopped"
            && s["batch_mining"]["entries"][0]["status"]["Failed"]["uncertain"] == false
    });
    assert_eq!(failed["batch_mining"]["entries"][0]["operation"], original);
    assert!(anki.collection().notes.is_empty());
    assert_eq!(add_count(&anki), 1);
    h.cmd(json!({"cmd":"batch-mining","action":"retry","index":0}));
    h.state_until("same operation retried", |s| {
        s["batch_mining"]["stage"] == "Done"
            && s["batch_mining"]["entries"][0]["status"]["Created"].is_u64()
    });
    h.cmd(json!({"cmd":"batch-mining","action":"continue"}));
    let done = h.state_until("remaining card created", |s| {
        s["batch_mining"]["stage"] == "Done"
            && s["batch_mining"]["entries"][1]["status"]["Created"].is_u64()
    });
    assert_eq!(anki.collection().notes.len(), 2);
    assert_eq!(add_count(&anki), 3);
    assert_eq!(done["word_status_undo"], false);
    assert!(anki.collection().notes.iter().any(|n| {
        n.fields
            .iter()
            .any(|(k, v)| k == "Sentence" && v.contains("が好き"))
    }));
    assert!(
        journal(&done)["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["status"]["Created"].is_u64())
    );
    // A fresh explicit batch is checked again; it cannot use Add Anyway implicitly.
    h.cmd(json!({"cmd":"batch-mining","action":"new"}));
    capture(&mut h, 0);
    capture(&mut h, 1);
    preview(&mut h);
    let before = add_count(&anki);
    h.cmd(json!({"cmd":"batch-mining","action":"start"}));
    let skipped = h.state_until("duplicates skipped", |s| {
        s["batch_mining"]["stage"] == "Done"
    });
    assert_eq!(skipped["batch_mining"]["entries"][0]["status"], "Duplicate");
    assert_eq!(skipped["batch_mining"]["entries"][1]["status"], "Duplicate");
    assert_eq!(add_count(&anki), before);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
// T-BMG-001, T-BMG-002, T-BMG-003, T-BMG-004: exact primary contexts, durable Sending before mutation and stop-after-current cancellation.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn selected_primary_batch_cancels_remaining_after_the_inflight_card() {
    let anki = FakeAnki::start().unwrap();
    let mut desk = Desktop::start();
    setup(&mut desk, &anki);
    let path = desk.path("batch.ja.srt");
    std::fs::write(&path,"1\n00:00:01,000 --> 00:00:02,000\n食べました。\n\n2\n00:00:03,000 --> 00:00:04,000\n猫です。\n").unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("first-frame");
    h.drop_files(&[path.to_str().unwrap()]);
    h.state_until("two primary cues", |s| s["subtitles"]["cues"] == 2);
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.state_until("paused source", |s| s["paused"] == true);
    h.cmd(json!({"cmd":"batch-mining","action":"words","words":[{"expression":"食べる","reading":"たべる"},{"expression":"猫","reading":"ねこ"}]}));
    let prepared = h.state_until("captured exact original contexts", |s| {
        s["batch_mining"]["stage"] == "Idle"
            && s["batch_mining"]["entries"]
                .as_array()
                .is_some_and(|e| e.len() == 2)
    });
    assert!(
        prepared["batch_mining"]["entries"][0]["contexts"][0]["sentence"] == "食べました。"
            || prepared["batch_mining"]["entries"][1]["contexts"][0]["sentence"] == "食べました。"
    );
    preview(&mut h);
    anki.on("addNote", Behaviour::ApplyAfter(Duration::from_millis(700)));
    h.cmd(json!({"cmd":"batch-mining","action":"start"}));
    until(Duration::from_secs(10), "first live add request", || {
        anki.requests()
            .iter()
            .any(|r| r["action"] == "addNote")
            .then_some(())
    });
    let sending = h.state();
    assert_eq!(sending["batch_mining"]["stage"], "Sending(0)");
    assert_eq!(journal(&sending)["items"][0]["status"], "Sending");
    h.cmd(json!({"cmd":"batch-mining","action":"cancel"}));
    let stopped = h.state_until("stop after current card", |s| {
        s["batch_mining"]["stage"] == "Stopped"
            && s["batch_mining"]["entries"][0]["status"]["Created"].is_u64()
            && s["batch_mining"]["entries"][1]["status"] == "Cancelled"
    });
    assert_eq!(anki.collection().notes.len(), 1);
    assert!(
        anki.collection().media.len() >= 2,
        "batch media were extracted and uploaded"
    );
    assert!(
        anki.collection().notes[0]
            .fields
            .iter()
            .any(|(k, v)| k == "SentenceAudio" && v.contains("[sound:"))
    );
    assert!(
        anki.collection().notes[0]
            .fields
            .iter()
            .any(|(k, v)| k == "Picture" && v.contains("<img"))
    );
    assert_eq!(add_count(&anki), 1);
    assert_eq!(stopped["paused"], true);
    assert_eq!(stopped["time_pos"], before["time_pos"]);
    h.cmd(json!({"cmd":"batch-mining","action":"continue"}));
    h.state_until("explicit remaining continuation", |s| {
        s["batch_mining"]["stage"] == "Done"
    });
    assert_eq!(anki.collection().notes.len(), 2);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
// T-BMG-004: a lost response is durable across restart; exact-tag reconciliation adopts one existing note without resend.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn unknown_batch_commits_survive_restart_and_reconcile_without_another_add() {
    let anki = FakeAnki::start().unwrap();
    let mut desk = Desktop::start();
    setup(&mut desk, &anki);
    let (mut app, mut h) = desk.launch();
    book(&desk, &mut h);
    capture(&mut h, 0);
    capture(&mut h, 1);
    preview(&mut h);
    anki.on("addNote", Behaviour::ApplyThenClose);
    h.cmd(json!({"cmd":"batch-mining","action":"start"}));
    let state = h.state_until("unknown outcome durable", |s| {
        s["batch_mining"]["stage"] == "Stopped"
            && s["batch_mining"]["entries"][0]["status"]["Failed"]["uncertain"] == true
    });
    let j = journal(&state);
    let id = j["id"].clone();
    let op = j["items"][0]["operation"].clone();
    assert_eq!(anki.collection().notes.len(), 1);
    assert_eq!(add_count(&anki), 1);
    h.cmd(json!({"cmd":"batch-mining","action":"retry","index":0}));
    assert_eq!(add_count(&anki), 1);
    assert_eq!(h.state()["batch_mining"]["stage"], "Stopped");
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"batch-mining","action":"open"}));
    h.state_until("unresolved journal reloaded", |s| {
        s["batch_mining"]["journals"]
            .as_array()
            .is_some_and(|j| j.len() == 1)
    });
    h.cmd(json!({"cmd":"batch-mining","action":"reconcile","batch":id,"operation":op}));
    let recovered = h.state_until("positive exact operation recovered", |s| {
        s["batch_mining"]["journals"][0]["items"][0]["status"]["Created"].is_u64()
    });
    assert_eq!(add_count(&anki), 1);
    assert_eq!(anki.collection().notes.len(), 1);
    assert_eq!(recovered["word_status_undo"], false);
    h.cmd(json!({"cmd":"batch-mining","action":"delete-journal","batch":id}));
    h.state_until("resolved history removed explicitly", |s| {
        s["batch_mining"]["journals"]
            .as_array()
            .is_some_and(Vec::is_empty)
    });
    assert_eq!(anki.collection().notes.len(), 1);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
