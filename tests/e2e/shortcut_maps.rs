// SPDX-License-Identifier: GPL-3.0-only

//! M113: shortcut maps exported, previewed, applied and kept in the actual app.
use super::support::{Desktop, exit_status, fixture};
use serde_json::json;
use std::time::Duration;

// T-KEY-008…T-KEY-010: export, a previewed import with an unknown action, a
// refused conflicting map, Apply changing live keys, and restart.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn shortcut_maps_are_exported_previewed_and_applied() {
    let mut desk = Desktop::start();
    desk.write_config(
        "[player]\nmpv_options = { ao = \"null\" }\n[shortcuts]\nmute = [\"KeyK\"]\n",
    );
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("first-frame");
    let out = desk.path("keys.json");
    h.cmd(json!({"cmd":"shortcuts","action":"export","path":out}));
    let text = std::fs::read_to_string(&out).unwrap();
    let mut map: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(map["format"], "nikomiru-shortcuts");
    assert_eq!(map["actions"]["mute"], json!(["KeyK"]));
    assert_eq!(map["actions"]["play_pause"][0], "Space");
    // the same map changes nothing
    h.cmd(json!({"cmd":"shortcuts","action":"import","path":out}));
    let same = h.state();
    assert_eq!(same["shortcuts"]["preview"]["changes"], json!([]));
    h.cmd(json!({"cmd":"shortcuts","action":"cancel"}));
    // a conflicting map is shown but cannot be applied
    map["actions"]["mute"] = json!(["Space"]);
    std::fs::write(&out, map.to_string()).unwrap();
    h.cmd(json!({"cmd":"shortcuts","action":"import","path":out}));
    let conflict = h.state();
    assert_eq!(conflict["shortcuts"]["preview"]["applicable"], false);
    assert!(
        h.try_cmd(json!({"cmd":"shortcuts","action":"apply"}))
            .is_err()
    );
    assert_eq!(h.state()["shortcuts"]["overrides"]["mute"], json!(["KeyK"]));
    // a newer version is refused outright
    std::fs::write(
        &out,
        r#"{"format":"nikomiru-shortcuts","version":9,"actions":{}}"#,
    )
    .unwrap();
    h.cmd(json!({"cmd":"shortcuts","action":"import","path":out}));
    let newer = h.state();
    assert!(newer["shortcuts"]["preview"].is_null());
    assert!(newer["shortcuts"]["status"].is_string());
    // a valid map with an action from the future
    map["actions"]["mute"] = json!(["shift+keyq", "KeyQ"]);
    map["actions"]["teleport"] = json!(["KeyT"]);
    std::fs::write(&out, map.to_string()).unwrap();
    h.cmd(json!({"cmd":"shortcuts","action":"import","path":out}));
    let preview = h.state()["shortcuts"]["preview"].clone();
    assert_eq!(preview["unknown"], json!(["teleport"]));
    assert_eq!(
        preview["changes"],
        json!([{"action":"mute","before":["KeyK"],"after":["Shift+KeyQ","KeyQ"]}])
    );
    let before = h.state()["muted"].clone();
    h.cmd(json!({"cmd":"shortcuts","action":"apply"}));
    let applied = h.state();
    assert!(applied["shortcuts"]["preview"].is_null());
    assert_eq!(
        applied["shortcuts"]["overrides"]["mute"],
        json!(["Shift+KeyQ", "KeyQ"])
    );
    h.key("KeyK");
    assert_eq!(h.state()["muted"], before, "the old key does nothing now");
    h.key("KeyQ");
    h.state_until("muted by the imported key", |s| s["muted"] != before);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    assert_eq!(
        h.state()["shortcuts"]["overrides"]["mute"],
        json!(["Shift+KeyQ", "KeyQ"]),
        "saved"
    );
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
