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

// T-SEC-004
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon, `cargo xtask fixtures` and the release build"]
fn the_release_build_starts_and_quits() {
    // `cargo xtask ci --full` builds it just before; by hand:
    // `cargo build --release -p nikomiru-app`
    let release =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/nikomiru");
    assert!(
        release.exists(),
        "{} is missing: cargo build --release -p nikomiru-app",
        release.display()
    );
    let mut desk = Desktop::start();
    let mut app = desk
        .command(&release, &[&fixture("av_basic.mkv")])
        .spawn()
        .unwrap();
    support::until(
        Duration::from_secs(20),
        "the release build loads a file",
        || {
            if let Some(status) = app.try_wait().unwrap() {
                panic!("the release build exited: {status}\n{}", desk.log());
            }
            desk.log().contains("file loaded").then_some(())
        },
    );
    let kill = std::process::Command::new("kill")
        .args(["-TERM", &app.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let status = exit_status(&mut app, Duration::from_secs(15));
    assert!(status.success(), "{status}\n{}", desk.log());
}

fn subs(state: &serde_json::Value) -> &serde_json::Value {
    &state["subtitles"]
}

fn visible_text(state: &serde_json::Value) -> Vec<String> {
    subs(state)["visible"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str().map(str::to_string))
        .collect()
}

// T-SUB-009
// T-SUB-012
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn embedded_subtitles_are_drawn_clickable_and_navigable() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    let shown = h.wait("cue-shown");
    assert_eq!(shown["index"], 0);
    let s = h.state();
    assert_eq!(visible_text(&s), ["食べました"]);
    // the Japanese SRT track, extracted and drawn by Nikomiru (SUB-007, SUB-008)
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("track:"),
        "{s}"
    );
    assert_eq!(subs(&s)["cues"], 3);

    // a click on the second grapheme selects it and pauses (byte offset 3: べ)
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 1 }));
    let s = h.state_until("the pause after the click", |s| s["paused"] == true);
    assert_eq!(subs(&s)["selected"], serde_json::json!([0, 3]));

    // Ctrl+→ goes 180 ms before the next cue
    h.cmd(serde_json::json!({ "cmd": "key", "code": "ArrowRight", "mods": ["Ctrl"] }));
    let s = h.state_until("the jump to the next cue", |s| {
        (time_pos(s) - 4.82).abs() < 0.05
    });
    assert!(visible_text(&s).is_empty(), "{s}");
    // F enters fullscreen, Esc leaves it (M1 checklist, item 10)
    h.key("KeyF");
    h.state_until("fullscreen", |s| s["fullscreen"] == true);
    h.key("Escape");
    h.state_until("windowed again", |s| s["fullscreen"] == false);
    // V turns the subtitles off and on again, keeping the track
    h.key("KeyV");
    h.state_until("subtitles off", |s| subs(s)["disabled"] == true);
    h.key("KeyV");
    let s = h.state_until("subtitles on", |s| subs(s)["disabled"] == false);
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("track:"),
        "{s}"
    );
    // X moves the subtitles 100 ms later
    h.key("KeyX");
    let s = h.state_until("the offset", |s| subs(s)["delay_ms"] == 100);
    assert!(s["toast"].as_str().unwrap_or("").contains("+100"), "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-006
// T-SUB-008
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_subtitle_file_next_to_the_video_wins_over_embedded_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("Ep01.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(
        dir.path().join("Ep01.ja.srt"),
        "1\n00:00:01,000 --> 00:00:03,000\n<i>外部</i>の字幕ファイル\n",
    )
    .unwrap();
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&video.to_string_lossy());
    h.wait("file-loaded");
    h.wait("cue-shown");
    let s = h.state();
    assert_eq!(visible_text(&s), ["外部の字幕ファイル"]);
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .ends_with("Ep01.ja.srt"),
        "{s}"
    );
    assert_eq!(subs(&s)["encoding"], "UTF-8");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PERF-07
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn perf_cues_appear_within_50_ms_of_their_start() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    for start in [2.0, 5.0, 8.0] {
        let shown = h.wait("cue-shown");
        let at = shown["time_pos"].as_f64().unwrap();
        assert!(
            (0.0..=0.050).contains(&(at - start)),
            "cue at {start} shown at {at}"
        );
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
