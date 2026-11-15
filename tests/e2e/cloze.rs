// SPDX-License-Identifier: GPL-3.0-only

//! M105: previewed own-source exercises, finite practice and explicit single-owner FSRS.
use super::support::{Desktop, Harness, exit_status};
use serde_json::{Value, json};
use std::time::Duration;
fn phrase(h: &mut Harness, text: &str, context: &str) -> String {
    h.cmd(json!({"cmd":"phrases","action":"new"}));
    h.cmd(json!({"cmd":"phrases","action":"edit","text":text,"context":context,"definition":"meaning"}));
    let id = h.state()["phrases"]["editor"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    h.cmd(json!({"cmd":"phrases","action":"save"}));
    h.state_until("committed phrase", |s| {
        s["phrases"]["busy"] == false
            && s["phrases"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["id"] == id)
    });
    id
}
fn exercise(h: &mut Harness, id: &str) {
    h.cmd(json!({"cmd":"cloze","action":"preview","id":id}));
    h.state_until("explicit preview", |s| {
        s["cloze"]["busy"] == false && s["cloze"]["preview"]["id"] == id
    });
    assert!(
        !h.state()["cloze"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["phrase"]["id"] == id)
    );
    h.cmd(json!({"cmd":"cloze","action":"create"}));
    h.state_until("exercise created", |s| {
        s["cloze"]["busy"] == false
            && s["cloze"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["phrase"]["id"] == id)
    });
}
fn row(state: &Value, id: &str) -> Value {
    state["cloze"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["phrase"]["id"] == id)
        .unwrap()
        .clone()
}
// T-CLO-001, T-CLO-002, T-CLO-003, T-CLO-004: actual preview/submit/undo and persistent FSRS with finite-mode isolation.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn own_phrase_clozes_separate_finite_practice_from_opt_in_schedules() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    h.state_until("notebook", |s| s["phrases"]["busy"] == false);
    let cat = phrase(&mut h, "猫", "猫が好きです。猫！");
    let dog = phrase(&mut h, "犬", "犬が好きです。");
    exercise(&mut h, &cat);
    exercise(&mut h, &dog);
    let before = h.state();
    h.cmd(json!({"cmd":"cloze","action":"start","ids":[cat],"mode":"typed"}));
    let first = h.state();
    assert_eq!(first["cloze"]["round"]["prompt"], "[…]が好きです。[…]！");
    assert!(first["cloze"]["round"]["answer"].is_null());
    h.cmd(json!({"cmd":"key","code":"Space","text_focus":true}));
    assert_eq!(h.state()["paused"], before["paused"]);
    h.cmd(json!({"cmd":"cloze","action":"input","text":"ねこ"}));
    assert!(h.state()["cloze"]["round"]["feedback"].is_null());
    h.cmd(json!({"cmd":"cloze","action":"submit"}));
    assert_eq!(h.state()["cloze"]["round"]["feedback"], false);
    h.cmd(json!({"cmd":"cloze","action":"next"}));
    assert_eq!(h.state()["cloze"]["round"]["results"], json!(["Incorrect"]));
    h.cmd(json!({"cmd":"cloze","action":"undo"}));
    assert_eq!(h.state()["cloze"]["round"]["index"], 0);
    h.cmd(json!({"cmd":"cloze","action":"input","text":" 猫 "}));
    h.cmd(json!({"cmd":"cloze","action":"submit"}));
    assert_eq!(h.state()["cloze"]["round"]["feedback"], true);
    h.cmd(json!({"cmd":"cloze","action":"next"}));
    assert_eq!(h.state()["cloze"]["round"]["index"], 1);
    assert!(row(&h.state(), &cat)["memory"].is_null());
    assert_eq!(h.state()["word_status_undo"], false);
    h.cmd(json!({"cmd":"cloze","action":"cancel"}));
    h.cmd(json!({"cmd":"cloze","action":"start","ids":[cat,dog],"mode":"choice"}));
    let options = h.state()["cloze"]["round"]["options"].clone();
    assert_eq!(options.as_array().unwrap().len(), 2);
    assert!(options.as_array().unwrap().contains(&json!("猫")));
    assert!(options.as_array().unwrap().contains(&json!("犬")));
    h.cmd(json!({"cmd":"cloze","action":"skip"}));
    h.cmd(json!({"cmd":"cloze","action":"skip"}));
    assert_eq!(
        h.state()["cloze"]["round"]["results"],
        json!(["Skipped", "Skipped"])
    );
    h.cmd(json!({"cmd":"cloze","action":"cancel"}));
    h.cmd(json!({"cmd":"cloze","action":"enrol","id":cat}));
    let enrolled = h.state_until("explicit enrolment", |s| {
        s["cloze"]["busy"] == false && row(s, &cat)["memory"].is_object()
    });
    assert_eq!(row(&enrolled, &cat)["memory"]["reps"], 0);
    h.cmd(json!({"cmd":"cloze","action":"start","scheduled":true}));
    h.cmd(json!({"cmd":"cloze","action":"input","text":"猫"}));
    h.cmd(json!({"cmd":"cloze","action":"submit"}));
    h.cmd(json!({"cmd":"cloze","action":"next","grade":"easy"}));
    let answered = h.state_until("single committed rating", |s| {
        s["cloze"]["busy"] == false && row(s, &cat)["memory"]["reps"] == 1
    });
    assert_eq!(answered["cloze"]["round"]["index"], 1);
    h.cmd(json!({"cmd":"cloze","action":"undo"}));
    h.state_until("schedule restored", |s| {
        s["cloze"]["busy"] == false
            && row(s, &cat)["memory"]["reps"] == 0
            && s["cloze"]["round"]["index"] == 0
    });
    h.cmd(json!({"cmd":"cloze","action":"input","text":"猫"}));
    h.cmd(json!({"cmd":"cloze","action":"submit"}));
    h.cmd(json!({"cmd":"cloze","action":"next","grade":"good"}));
    let saved = h.state_until("schedule committed again", |s| {
        s["cloze"]["busy"] == false && row(s, &cat)["memory"]["reps"] == 1
    });
    let memory = row(&saved, &cat)["memory"].clone();
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"cloze","action":"open"}));
    let loaded = h.state_until("exercise schedules survive restart", |s| {
        s["cloze"]["busy"] == false && s["cloze"]["items"].as_array().is_some_and(|r| r.len() == 2)
    });
    assert_eq!(row(&loaded, &cat)["memory"], memory);
    assert!(row(&loaded, &dog)["memory"].is_null());
    assert_eq!(loaded["word_status_undo"], false);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
