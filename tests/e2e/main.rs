// SPDX-License-Identifier: GPL-3.0-only

//! L6: the real `nikomiru` binary, built with the `test-harness` feature and
//! driven through the GUI test harness in an off-screen KWin (TEST §2).
//! `cargo xtask ci --full` runs them; by hand:
//! `cargo test -p nikomiru-app --features test-harness --test e2e -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use std::time::Duration;

use support::{Desktop, exit_status, fixture};

const NEEDS: &str = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`";

fn time_pos(state: &serde_json::Value) -> f64 {
    state["time_pos"].as_f64().unwrap_or(-1.0)
}

// T-E2E-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn open_play_seek_close_and_reopen_resumes() {
    let _ = NEEDS;
    let video = fixture("av_long.mkv");
    let mut desk = Desktop::start();

    let (mut app, mut h) = desk.launch();
    h.open(&video);
    h.wait("file-loaded");
    h.wait("first-frame");
    let playing = h.state_until("playback", |s| s["paused"] == false && time_pos(s) > 0.0);
    assert_eq!(playing["title"], "av_long.mkv");

    // seek forward from the keyboard: ArrowRight = +3 s by default (PLY-001)
    let before = time_pos(&h.state());
    h.key("ArrowRight");
    h.state_until("the seek", |s| time_pos(s) >= before + 2.5);

    // pause from the keyboard, then close the window
    h.key("Space");
    let paused = h.state_until("the pause", |s| s["paused"] == true);
    let saved = time_pos(&paused);
    assert!(saved > 3.0, "position {saved}");
    h.quit();
    let status = exit_status(&mut app, Duration::from_secs(15));
    assert!(status.success(), "{status}\n{}", desk.log());

    // reopen: the file resumes from the saved position (LIB-006, PLY-007)
    let (mut app, mut h) = desk.launch();
    h.open(&video);
    h.wait("file-loaded");
    let resumed = h.state_until("the resume position", |s| time_pos(s) > 0.0);
    let at = time_pos(&resumed);
    assert!(
        (saved - 0.5..saved + 2.0).contains(&at),
        "saved at {saved}, resumed at {at}\n{}",
        desk.log()
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-APP-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_second_launch_opens_its_file_in_the_first() {
    let video = fixture("av_basic.mkv");
    let mut desk = Desktop::start();
    let (mut first, mut h) = desk.launch();
    assert_eq!(h.state()["loaded"], false);

    let mut second = desk.app(&[&video]).spawn().unwrap();
    let status = exit_status(&mut second, Duration::from_secs(15));
    assert!(status.success(), "second launch: {status}\n{}", desk.log());

    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "av_basic.mkv");
    h.quit();
    assert!(exit_status(&mut first, Duration::from_secs(15)).success());
}

// T-LIB-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn files_open_from_the_command_line_and_by_drag_and_drop() {
    let basic = fixture("av_basic.mkv");
    let weird = fixture("-weird: name & spaces ü 日本.mkv");
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch_with(&["--", &basic]);
    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "av_basic.mkv");

    // a drop arrives as percent-encoded file:// URLs
    h.drop_files(&[&weird]);
    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "-weird: name & spaces ü 日本.mkv");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PRV-002
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_logs_never_hold_an_api_key() {
    const SENTINEL: &str = "SENTINEL-4f2a9c";
    let mut desk = Desktop::start();
    // a key in its place and one in the wrong field, which makes the file invalid
    desk.write_config(&format!(
        "[anki]\napi_key = \"{SENTINEL}-key\"\n[player]\nseek_seconds = \"{SENTINEL}-misplaced\"\n"
    ));
    let (mut app, mut h) = desk.launch_with(&["--debug"]);
    let error = h.state()["error"].as_str().unwrap_or_default().to_string();
    assert!(
        error.contains("config.toml"),
        "the invalid settings are reported: {error:?}"
    );
    assert!(!error.contains(SENTINEL), "{error}");
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let logs = desk.log();
    assert!(logs.contains("Nikomiru"), "the logs were written");
    assert!(
        !logs.contains(SENTINEL),
        "a secret reached the logs:\n{logs}"
    );
}
