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
