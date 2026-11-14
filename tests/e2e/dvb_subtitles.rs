// SPDX-License-Identifier: GPL-3.0-only

//! SUB-032: actual embedded DVB bitmap subtitles through local OCR.

use super::support::{Desktop, exit_status, fixture};
use nikomiru_subs::{pgs::testing::block, vobsub::testing as dvd};
use serde_json::json;
use std::time::Duration;

// T-SUB-032: the player offers both pictures/read, converts a real DVB stream
// and exposes its text for dictionary lookup without a new model/network.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and ffmpeg"]
fn embedded_dvb_subtitles_are_offered_and_read_as_text() {
    let mut desk = Desktop::start();
    let idx = desk.path("input.idx");
    let (bytes, starts) = dvd::program_stream(
        &[
            (1000, dvd::spu(20, 20, &block(200, 40), 1000)),
            (4000, dvd::spu(20, 20, &block(240, 40), 1000)),
        ],
        2000,
    );
    std::fs::write(&idx, dvd::idx(&dvd::PALETTE, &[1000, 4000], &starts)).unwrap();
    std::fs::write(idx.with_extension("sub"), bytes).unwrap();
    let video = desk.path("broadcast.ts");
    let result = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-copyts", "-i"])
        .arg(fixture("av_basic.mkv"))
        .arg("-i")
        .arg(&idx)
        .args([
            "-map",
            "0:v",
            "-map",
            "0:a",
            "-map",
            "1:s:0",
            "-c:v",
            "copy",
            "-c:a",
            "copy",
            "-c:s",
            "dvbsub",
            "-metadata:s:s:0",
            "language=jpn",
            "-f",
            "mpegts",
        ])
        .arg(&video)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    let s = h.state_until("DVB read as text", |s| {
        s["subtitles"]["cues"] == 2 && s["subtitles"]["incomplete"] == false
    });
    assert!(
        s["subtitles"]["source"]
            .as_str()
            .unwrap()
            .starts_with("read:")
    );
    let menu = s["subtitles"]["menu"].as_array().unwrap();
    assert!(menu.iter().any(|t| t["kind"] == "bitmap"));
    assert!(
        menu.iter()
            .any(|t| t["kind"] == "read" && t["codec"] == "dvb_subtitle")
    );
    h.cmd(json!({"cmd":"click_grapheme","layer":"main","cue_index":0,"grapheme_index":3}));
    h.state_until("DVB text lookup", |s| {
        s["popups"][0]["query"] == "食べました"
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
