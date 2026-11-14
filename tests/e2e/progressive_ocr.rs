// SPDX-License-Identifier: GPL-3.0-only

//! SUB-030: clickable partial OCR, completion and stale-source rejection.

use super::support::{Desktop, exit_status, fixture};
use nikomiru_subs::pgs::testing;
use serde_json::json;
use std::time::Duration;

// T-SUB-030: partial text is usable before the whole track, without a partial
// comprehension/export; open lookup survives later snapshots and completion.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn graphic_text_arrives_progressively_and_old_progress_cannot_replace_a_file() {
    let mut desk = Desktop::start();
    desk.set_env("NIKOMIRU_TEST_OCR_DELAY_MS", "250");
    desk.write_config("[subtitles]\nword_colors = true\n");
    let mut sup = Vec::new();
    for i in 0..64 {
        testing::display_set(
            &mut sup,
            1000 + i * 1500,
            &[(100, 900, testing::block(80 + i as usize, 20))],
            255,
        );
        testing::display_set(&mut sup, 2000 + i * 1500, &[], 255);
    }
    let input = desk.path("input.sup");
    std::fs::write(&input, &sup).unwrap();
    let video = desk.path("graphic.mkv");
    assert!(
        std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-loglevel", "error", "-copyts", "-i"])
            .arg(fixture("av_long.mkv"))
            .arg("-i")
            .arg(&input)
            .args([
                "-map",
                "0:v",
                "-map",
                "0:a",
                "-map",
                "1:0",
                "-c",
                "copy",
                "-metadata:s:s:0",
                "language=jpn"
            ])
            .arg(&video)
            .status()
            .unwrap()
            .success()
    );
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    let s = h.state_until("partial OCR", |s| {
        s["subtitles"]["incomplete"] == true
            && s["subtitles"]["cues"]
                .as_u64()
                .is_some_and(|n| n > 0 && n < 64)
    });
    // T-SUB-033: cumulative picture progress is visible only while incomplete.
    assert_eq!(s["subtitles"]["read_progress"][1], 64);
    assert!(s["subtitles"]["read_progress"][0].as_u64().unwrap() > 0);
    assert!(s["subtitles"]["score"].is_null());
    // T-SUB-034: partial OCR never exports an incomplete SRT.
    let partial = desk.path("partial.srt");
    h.cmd(json!({"cmd":"subtitle-export", "path":partial}));
    assert!(!partial.exists());
    h.cmd(json!({"cmd":"condense","format":"opus","folder":desk.path("not-exported")}));
    assert_eq!(h.state()["audio_export"]["running"], false);
    h.cmd(json!({"cmd":"click_grapheme","layer":"main","cue_index":0,"grapheme_index":3}));
    h.state_until("lookup in partial OCR", |s| {
        s["popups"][0]["query"] == "食べました"
    });
    let s = h.state_until("full OCR retains its popup", |s| {
        s["subtitles"]["incomplete"] == false && s["subtitles"]["cues"] == 64
    });
    assert!(s["subtitles"]["read_progress"].is_null());
    assert_eq!(s["popups"][0]["query"], "食べました");
    let exported = desk.path("complete.srt");
    h.cmd(json!({"cmd":"subtitle-export", "path":exported}));
    let text = std::fs::read_to_string(&exported).unwrap();
    let parsed = nikomiru_subs::parse(nikomiru_subs::Format::Srt, &text);
    assert_eq!(parsed.cues.len(), 64);
    assert!(parsed.cues[0].plain_text.contains("食べました"));
    h.key("Escape");
    let another = desk.path("another.mkv");
    std::fs::copy(&video, &another).unwrap();
    h.open(another.to_str().unwrap());
    h.wait("file-loaded");
    h.state_until("another partial OCR request", |s| {
        s["subtitles"]["incomplete"] == true
            && s["subtitles"]["cues"]
                .as_u64()
                .is_some_and(|n| n > 0 && n < 64)
    });
    // T-SUB-033: explicit cancellation returns to bitmap and rejects late batches.
    h.cmd(json!({"cmd":"graphic-cancel"}));
    h.state_until("cancelled graphic reading restores pictures", |s| {
        s["subtitles"]["incomplete"] == false
            && s["subtitles"]["cues"] == 0
            && s["subtitles"]["source"]
                .as_str()
                .is_some_and(|s| s.starts_with("track:"))
            && s["subtitles"]["read_progress"].is_null()
    });
    // A replacement source keeps its text when an older graphic request has
    // queued progress/completion. The existing cache makes this path fast too.
    let text = desk.path("replacement.srt");
    std::fs::write(&text, "1\n00:00:00,000 --> 00:02:00,000\n日本語\n").unwrap();
    h.drop_files(&[text.to_str().unwrap()]);
    h.state_until("replacement text", |s| {
        s["subtitles"]["cues"] == 1 && s["subtitles"]["incomplete"] == false
    });
    std::thread::sleep(Duration::from_millis(750));
    let s = h.state();
    assert_eq!(s["subtitles"]["cues"], 1);
    assert_eq!(s["subtitles"]["incomplete"], false);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let cache = desk.path("cache/nikomiru/subtitles");
    assert!(
        nikomiru_app::graphic::cache_path(&cache, &video, 2)
            .unwrap()
            .is_file()
    );
    assert!(
        !nikomiru_app::graphic::cache_path(&cache, &another, 2)
            .unwrap()
            .is_file()
    );
}
