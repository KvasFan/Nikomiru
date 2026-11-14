// SPDX-License-Identifier: GPL-3.0-only

//! M46…M55: complete subtitle snapshots and study actions in the real application.

use super::support::{Desktop, exit_status, fixture};
use serde_json::json;
use std::time::Duration;

// T-SUB-035, T-SUB-037: selected, invalid and stale snapshot requests use the real save handler.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn selected_subtitle_snapshots_keep_timing_and_reject_stale_requests() {
    let mut desk = Desktop::start();
    let video = desk.path("study.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    let subtitles = desk.path("study.ja.srt");
    std::fs::write(&subtitles, "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:06,000\n犬\n\n3\n00:00:08,000 --> 00:00:10,000\n鳥\n").unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.drop_files(&[subtitles.to_str().unwrap()]);
    h.state_until("three text cues", |s| {
        s["subtitles"]["cues"] == 3 && s["subtitles"]["incomplete"] == false
    });
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.state();
    let srt = desk.path("selected.srt");
    h.cmd(json!({"cmd":"subtitle-export", "path":srt, "indices":[1]}));
    let parsed = nikomiru_subs::parse(
        nikomiru_subs::Format::Srt,
        &std::fs::read_to_string(srt).unwrap(),
    );
    assert_eq!(parsed.cues.len(), 1);
    assert_eq!(
        (parsed.cues[0].start_ms, parsed.cues[0].end_ms),
        (3000, 6000)
    );
    assert_eq!(parsed.cues[0].plain_text, "犬");
    let vtt = desk.path("selected.vtt");
    h.cmd(json!({"cmd":"subtitle-export", "path":vtt, "indices":[2,0,2], "format":"vtt"}));
    let parsed = nikomiru_subs::parse(
        nikomiru_subs::Format::Vtt,
        &std::fs::read_to_string(vtt).unwrap(),
    );
    assert_eq!(
        parsed
            .cues
            .iter()
            .map(|c| c.plain_text.as_str())
            .collect::<Vec<_>>(),
        ["猫", "鳥"]
    );
    let stale = desk.path("stale.vtt");
    h.cmd(json!({"cmd":"subtitle-export", "path":stale, "indices":[0], "format":"vtt", "version":u64::MAX}));
    assert!(!stale.exists());
    let invalid = desk.path("invalid.srt");
    h.cmd(json!({"cmd":"subtitle-export", "path":invalid, "indices":[0,9]}));
    assert!(!invalid.exists());
    let after = h.state();
    assert_eq!(
        before["subtitles"]["version"],
        after["subtitles"]["version"]
    );
    assert_eq!(after["paused"], true);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
