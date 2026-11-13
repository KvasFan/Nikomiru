// SPDX-License-Identifier: GPL-3.0-only

//! PLY-022: the real player, temporary speed and saved episode settings.

use std::time::Duration;

use serde_json::json;

use crate::support::{Desktop, Harness, exit_status, fixture};
use crate::time_pos;

fn speed(h: &mut Harness, want: f64) {
    h.state_until("the playback speed", |s| {
        s["speed"].as_f64().is_some_and(|v| (v - want).abs() < 0.01)
    });
    let actual = h.cmd(json!({ "cmd": "mpv_property", "name": "speed" }));
    assert_eq!(actual.as_str().unwrap().parse::<f64>().unwrap(), want);
}

// T-PLY-022
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn gap_speed_restores_manual_choices_and_never_saves_the_override() {
    let mut desk = Desktop::start();
    desk.write_config("[player]\ngap_speed = 4\nmpv_options = { ao = \"null\" }\n");
    let video = desk.path("gap.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    std::fs::write(
        video.with_extension("ja.srt"),
        "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n\
         2\n00:01:00,000 --> 00:01:01,000\n犬\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    speed(&mut h, 4.0);
    h.key("Space");
    h.state_until("paused", |s| s["paused"] == true);
    speed(&mut h, 1.0);
    h.key("Equal");
    speed(&mut h, 1.1);
    h.key("Space");
    speed(&mut h, 4.0);
    // The speed-up key changes the underlying 1.1×, not the temporary 4×.
    h.key("Equal");
    h.cmd(json!({
        "cmd": "settings", "tab": "general",
        "save": "[player]\ngap_speed = 0\n",
    }));
    speed(&mut h, 1.2);
    h.cmd(json!({
        "cmd": "settings", "tab": "general",
        "save": "[player]\ngap_speed = 4\n",
    }));
    h.cmd(json!({ "cmd": "settings", "tab": "close" }));
    speed(&mut h, 4.0);
    // A lookup of the first line from the list suspends acceleration; its
    // replay keeps the chosen speed and stops at the sentence's own end.
    h.cmd(json!({
        "cmd": "click_grapheme", "cue_index": 0, "grapheme_index": 0,
    }));
    h.state_until("the popup pause", |s| s["paused"] == true);
    speed(&mut h, 1.2);
    h.cmd(json!({ "cmd": "popup_button", "button": "replay" }));
    h.state_until("the replay", |s| s["paused"] == false && time_pos(s) < 2.0);
    speed(&mut h, 1.2);
    h.state_until("the replay's end", |s| {
        s["paused"] == true && time_pos(s) > 1.8
    });
    h.key("Escape");
    speed(&mut h, 4.0);
    // Even a press and release delivered together cannot restore 4× as the
    // user's speed. Disable before the next gap update and check its base.
    h.key("KeyE");
    h.cmd(json!({
        "cmd": "settings", "tab": "general",
        "save": "[player]\ngap_speed = 0\n",
    }));
    speed(&mut h, 1.2);
    h.cmd(json!({
        "cmd": "settings", "tab": "general",
        "save": "[player]\ngap_speed = 4\n",
    }));
    h.cmd(json!({ "cmd": "settings", "tab": "close" }));
    speed(&mut h, 4.0);
    let continuous = desk.path("continuous.srt");
    std::fs::write(&continuous, "1\n00:00:00,000 --> 00:02:00,000\n猫\n").unwrap();
    h.drop_files(&[continuous.to_str().unwrap()]);
    h.state_until("the replacement subtitle", |s| s["subtitles"]["cues"] == 1);
    speed(&mut h, 1.2);
    h.drop_files(&[video.with_extension("ja.srt").to_str().unwrap()]);
    speed(&mut h, 4.0);
    h.open(&fixture("av_noaudio.mp4"));
    h.wait("file-loaded");
    speed(&mut h, 1.2);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let lib = nikomiru_library::Library::open(&desk.data_file("library.redb")).unwrap();
    let uid = lib.uid_for_path(&video).unwrap();
    assert_eq!(lib.video(&uid).unwrap().unwrap().speed, Some(1.2));
}

// AK-BUG-006, T-PLY-022, T-SUB-026
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn gap_speed_restores_dialogue_and_keeps_the_pause_at_each_line() {
    let mut desk = Desktop::start();
    desk.write_config(
        "[player]\ngap_speed = 4\nmpv_options = { ao = \"null\" }\n\
         [subtitles]\nauto_pause = true\n",
    );
    let video = desk.path("dialogue.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(
        video.with_extension("ja.srt"),
        "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n\
         2\n00:00:07,000 --> 00:00:08,000\n犬\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    let s = h.state_until("the first line's stop", |s| {
        s["paused"] == true && time_pos(s) > 1.5
    });
    assert!((time_pos(&s) - 2.0).abs() < 0.15, "{s}");
    speed(&mut h, 1.0);
    h.key("Space");
    speed(&mut h, 4.0);
    let mut samples = Vec::new();
    let restored = crate::support::until(Duration::from_secs(10), "the next line's lead", || {
        let s = h.state();
        samples.push((time_pos(&s), s["speed"].clone(), s["paused"].clone()));
        if time_pos(&s) > 6.0 && time_pos(&s) < 7.0 && s["speed"] == 1.0 {
            Some(true)
        } else if time_pos(&s) >= 7.0 {
            Some(false)
        } else {
            None
        }
    });
    assert!(restored, "the speed must return before speech: {samples:?}");
    let s = h.state_until("the second line's stop", |s| {
        s["paused"] == true && time_pos(s) > 7.5
    });
    assert!((time_pos(&s) - 8.0).abs() < 0.15, "{s}");
    h.key("Space");
    h.state_until("the closing stretch", |s| time_pos(s) > 9.0);
    speed(&mut h, 1.0);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
