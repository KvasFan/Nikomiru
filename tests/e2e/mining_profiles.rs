// SPDX-License-Identifier: GPL-3.0-only

//! M108: explicit profiles, literal field pins, sense controls and draft ownership.
use super::support::{Desktop, Harness, exit_status, until};
use nikomiru_anki::fake::FakeAnki;
use serde_json::json;
use std::time::Duration;
fn setup(desk: &Desktop, fake: &FakeAnki) {
    desk.import_dictionaries(&["mini-ja"]);
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&fake.url(), "").unwrap()).unwrap();
    desk.write_config(&format!("[anki]\nurl={:?}\n", fake.url()));
}
fn open_book(desk: &Desktop, h: &mut Harness, name: &str, text: &str) {
    let path = desk.path(name);
    std::fs::write(&path, text).unwrap();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    h.state_until("composer source", |s| {
        s["books"]["open"] == name.trim_end_matches(".txt")
    });
}
fn compose(h: &mut Harness, paragraph: usize) {
    if !h.state()["popups"].as_array().unwrap().is_empty() {
        h.key("Escape");
    }
    h.cmd(json!({"cmd":"books","action":"click","paragraph":paragraph,"offset":0}));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"card","level":0,"entry":0,"item":"advanced"}));
    h.state_until("composer fields and definitions", |s| {
        s["advanced"]["available_fields"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
            && !s["anki_page"]["busy"].as_bool().unwrap_or(true)
    });
    h.cmd(json!({"cmd":"advanced","edit":"frame-none"}));
}
fn field(fake: &FakeAnki, index: usize, name: &str) -> String {
    fake.collection().notes[index]
        .fields
        .iter()
        .find(|(k, _)| k == name)
        .unwrap()
        .1
        .clone()
}
// T-MCP-001…T-MCP-004: pins are explicit, scope resets on profile application, and quick mining never inherits them.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn named_profiles_and_literal_pins_survive_only_their_explicit_scope() {
    let fake = FakeAnki::start().unwrap();
    let mut desk = Desktop::start();
    setup(&desk, &fake);
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"mining-profile","action":"save","name":"Reading"}));
    h.state_until("profile saved", |s| {
        s["anki_page"]["profiles"][0] == "Reading"
    });
    open_book(
        &desk,
        &mut h,
        "Composer.txt",
        "猫です。\n\n犬です。\n\n食べました。",
    );
    compose(&mut h, 0);
    h.cmd(json!({"cmd":"composer","action":"field","field":"Sentence","text":"My <literal> {expression}"}));
    h.cmd(json!({"cmd":"composer","action":"pin","field":"Sentence","on":true}));
    h.cmd(json!({"cmd":"composer","action":"sense","index":0,"on":false}));
    h.cmd(json!({"cmd":"advanced","edit":"create"}));
    h.state_until("empty sense selection retained", |s| {
        s["advanced"]["error"].is_string()
    });
    assert!(fake.collection().notes.is_empty());
    h.cmd(json!({"cmd":"composer","action":"sense","index":0,"on":true}));
    h.cmd(json!({"cmd":"advanced","edit":"create"}));
    h.cmd(json!({"cmd":"advanced","edit":"create"}));
    until(Duration::from_secs(10), "first explicit card", || {
        (fake.collection().notes.len() == 1).then_some(())
    });
    assert_eq!(
        field(&fake, 0, "Sentence"),
        "My &lt;literal&gt; {expression}"
    );
    compose(&mut h, 1);
    let state = h.state();
    assert_eq!(
        state["advanced"]["fields"]["Sentence"],
        "My <literal> {expression}"
    );
    assert_eq!(state["advanced"]["pinned"][0], "Sentence");
    assert!(
        state["advanced"]["senses"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["selected"] == true)
    );
    h.cmd(json!({"cmd":"advanced","edit":"create"}));
    until(
        Duration::from_secs(10),
        "second fresh word with pin",
        || (fake.collection().notes.len() == 2).then_some(()),
    );
    assert_eq!(field(&fake, 1, "Expression"), "犬");
    assert_eq!(field(&fake, 1, "Sentence"), field(&fake, 0, "Sentence"));
    compose(&mut h, 2);
    h.cmd(json!({"cmd":"mining-profile","action":"apply","name":"Reading"}));
    h.state_until("profile explicitly discards composer", |s| {
        s["advanced"].is_null() && s["anki_page"]["active_profile"] == "Reading"
    });
    h.state_until("profile checked", |s| s["anki_page"]["busy"] == false);
    // Quick mining uses the original current source, never the previous pinned sentence.
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    until(Duration::from_secs(10), "ordinary card", || {
        (fake.collection().notes.len() == 3).then_some(())
    });
    assert!(field(&fake, 2, "Sentence").contains("食べました"));
    assert!(!field(&fake, 2, "Sentence").contains("literal"));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(10)).success());
    let (mut app, mut h) = desk.launch();
    h.state_until("profile retained after restart", |s| {
        s["anki_page"]["profiles"][0] == "Reading"
    });
    open_book(&desk, &mut h, "After.txt", "見る。");
    compose(&mut h, 0);
    assert_eq!(h.state()["advanced"]["fields"], json!({}));
    h.cmd(json!({"cmd":"advanced","edit":"cancel"}));
    h.cmd(json!({"cmd":"mining-profile","action":"delete","name":"Reading"}));
    h.state_until("explicit profile deletion", |s| {
        s["anki_page"]["profiles"].as_array().unwrap().is_empty()
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(10)).success());
}
// T-MCP-001, T-MCP-004: a new source and missing model never silently create against the original captured draft.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn stale_composer_sources_and_missing_profile_models_are_refused() {
    let fake = FakeAnki::start().unwrap();
    let mut desk = Desktop::start();
    setup(&desk, &fake);
    let (mut app, mut h) = desk.launch();
    open_book(&desk, &mut h, "Original.txt", "猫です。");
    compose(&mut h, 0);
    open_book(&desk, &mut h, "Replaced.txt", "犬です。");
    h.cmd(json!({"cmd":"advanced","edit":"create"}));
    let state = h.state();
    assert!(
        state["advanced"].is_null() || state["advanced"]["error"].is_string(),
        "{state}"
    );
    assert!(fake.collection().notes.is_empty());
    h.cmd(json!({"cmd":"advanced","edit":"cancel"}));
    h.cmd(json!({"cmd":"anki","action":"save","config":format!("url={:?}\nnote_type='Absent'\n",fake.url())}));
    h.cmd(json!({"cmd":"mining-profile","action":"save","name":"Unavailable"}));
    h.cmd(json!({"cmd":"mining-profile","action":"apply","name":"Unavailable"}));
    h.state_until("missing profile model reported", |s| {
        s["anki_page"]["status"]["ok"] == false && s["anki_page"]["busy"] == false
    });
    assert!(!fake.collection().models.iter().any(|m| m.name == "Absent"));
    assert!(fake.collection().notes.is_empty());
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(10)).success());
}
