// SPDX-License-Identifier: GPL-3.0-only

//! M117: touch gestures in the actual app (PORT-003).
use super::support::{Desktop, exit_status, fixture};
use serde_json::json;
use std::time::Duration;

// T-PORT-004: swipes seek a paused video and turn the reader's pages; a vertical
// drag does neither.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn swipes_seek_the_video_and_turn_pages() {
    let mut desk = Desktop::start();
    desk.write_config(
        "[player]\nmpv_options = { ao = \"null\" }\nseek_seconds = 5\n[books]\nmode = \"pages\"\n",
    );
    let book = desk.path("Long.txt");
    let text: String = (0..80).map(|n| format!("段落{n}です。\n\n")).collect();
    std::fs::write(&book, text).unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("first-frame");
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.paused_settled("paused")["time_pos"].as_f64().unwrap();
    h.cmd(json!({"cmd":"touch","from":[300.0,300.0],"to":[600.0,310.0]}));
    let forward = h.state_until("a swipe right seeks forward", |s| {
        s["time_pos"].as_f64().is_some_and(|t| t >= before + 4.0)
    })["time_pos"]
        .as_f64()
        .unwrap();
    h.cmd(json!({"cmd":"touch","from":[600.0,200.0],"to":[610.0,500.0]}));
    std::thread::sleep(Duration::from_millis(300));
    let after = h.state()["time_pos"].as_f64().unwrap();
    assert!(
        (after - forward).abs() < 1.0,
        "a vertical drag does not seek"
    );
    h.cmd(json!({"cmd":"touch","from":[600.0,300.0],"to":[300.0,300.0]}));
    h.state_until("a swipe left seeks back", |s| {
        s["time_pos"].as_f64().is_some_and(|t| t <= forward - 4.0)
    });
    // the reader in Pages mode
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":book}));
    h.state_until("reader", |s| {
        s["books"]["open"] == "Long" && s["books"]["paragraph"] == 0
    });
    h.cmd(json!({"cmd":"touch","from":[700.0,400.0],"to":[300.0,400.0]}));
    let turned = h.state_until("next page", |s| {
        s["books"]["paragraph"].as_u64().is_some_and(|p| p > 0)
    });
    let page = turned["books"]["paragraph"].as_u64().unwrap();
    h.cmd(json!({"cmd":"touch","from":[300.0,400.0],"to":[700.0,400.0]}));
    h.state_until("previous page", |s| s["books"]["paragraph"] == 0);
    assert!(page > 0);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-PLY-024: on a narrow window (a phone upright) the bar keeps play, the time
// and the subtitles, and its "more" menu reaches the rest.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_narrow_bar_puts_the_rest_in_a_menu() {
    let mut desk = Desktop::start();
    // 200 %: the 1280-pixel window is 640 points across
    desk.write_config("[general]\nui_scale = 200\n[player]\nmpv_options = { ao = \"null\" }\n");
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("first-frame");
    let state = h.state_until("a narrow bar", |s| s["bar"]["compact"] == true);
    let more = state["bar"]["more"].as_array().unwrap().clone();
    let at = |r: &[serde_json::Value]| {
        let n = |i: usize| r[i].as_f64().unwrap();
        ((n(0) + n(2)) / 2.0, (n(1) + n(3)) / 2.0)
    };
    let (x, y) = at(&more);
    h.cmd(json!({"cmd":"button","x":x,"y":y,"pressed":true}));
    h.cmd(json!({"cmd":"button","x":x,"y":y,"pressed":false}));
    h.state_until("the menu", |s| {
        s["bar"]["menu"]
            .as_object()
            .is_some_and(|m| m.contains_key("Settings"))
    });
    // egui lays a new menu out once unseen: its place is the next frame's
    std::thread::sleep(Duration::from_millis(300));
    let open = h.state();
    let names: Vec<String> = open["bar"]["menu"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    for name in ["Settings", "Anki", "Open file", "Fullscreen"] {
        assert!(
            names.iter().any(|n| n.contains(name)),
            "{name} in {names:?}"
        );
    }
    let (x, y) = at(open["bar"]["menu"]["Settings"].as_array().unwrap());
    h.cmd(json!({"cmd":"button","x":x,"y":y,"pressed":true}));
    h.cmd(json!({"cmd":"button","x":x,"y":y,"pressed":false}));
    h.state_until("the settings", |s| s["settings_open"] == true);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
