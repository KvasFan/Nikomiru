// SPDX-License-Identifier: GPL-3.0-only

//! AUD-010: export captures the current player's subtitles and audio, even
//! when another file is opened before ffmpeg finishes.

use crate::support::{Desktop, exit_status, fixture};
use serde_json::json;
use std::time::Duration;

// T-AUD-010
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn condensed_audio_keeps_the_episode_chosen_before_another_file_opens() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch_with(&["--", &fixture("av_basic.mkv")]);
    h.wait("first-frame");
    h.state_until("the main subtitles", |s| s["subtitles"]["cues"] == 3);
    h.key("Space");
    h.state_until("paused", |s| s["paused"] == true);
    h.key("KeyX");
    h.state_until("the subtitle offset", |s| s["subtitles"]["delay_ms"] == 100);
    let folder = desk.path("exported-audio");
    h.cmd(json!({"cmd":"condense","format":"opus","folder":folder}));
    h.open(&fixture("av_noaudio.mp4"));
    h.wait("file-loaded");
    let report = h.state_until("the completed audio export", |s| {
        s["audio_export"]["written"].is_array() && s["audio_export"]["running"] == false
    });
    assert!(
        report["audio_export"]["failed"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{report}"
    );
    let written = report["audio_export"]["written"].as_array().unwrap();
    assert_eq!(written.len(), 1);
    let path = written[0].as_str().unwrap();
    assert!(path.ends_with(".opus"));
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_chapters", "-of", "json", path])
        .output()
        .unwrap();
    assert!(probe.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(
        metadata["chapters"].as_array().unwrap().len(),
        3,
        "{metadata}"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
