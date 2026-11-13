// SPDX-License-Identifier: GPL-3.0-only

//! SUB-031: explicit local VobSub pair OCR through the actual loader.

use super::support::{Desktop, exit_status, fixture};
use nikomiru_subs::{pgs::testing::block, vobsub::testing as dvd};
use serde_json::json;
use std::time::Duration;

// T-SUB-031: either member, preferred language, source invalidation and no
// partial cache on malformed/missing companions.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn external_vobsub_pairs_are_read_and_changed_companions_are_not_cached_as_old_text() {
    let mut desk = Desktop::start();
    desk.write_config("[subtitles]\npreferred_languages = [\"ja\"]\n");
    let idx = desk.path("external.IDX");
    let sub = desk.path("external.SUB");
    let (mut bytes, en) =
        dvd::program_stream(&[(1000, dvd::spu(0, 0, &block(100, 20), 1000))], 2000);
    let offset = bytes.len();
    let (jp, starts) = dvd::program_stream_track(
        &[
            (5000, dvd::spu(0, 0, &block(200, 20), 1000)),
            (7000, dvd::spu(0, 0, &block(300, 20), 1000)),
        ],
        2000,
        3,
    );
    bytes.extend(jp);
    std::fs::write(&sub, &bytes).unwrap();
    let mut index =
        dvd::idx(&dvd::PALETTE, &[1000], &en).replace("id: ja, index: 0", "id: en, index: 0");
    index.push_str(&format!("id: ja, index: 3\ntimestamp: 00:00:02:000, filepos: {:09x}\ntimestamp: 00:00:04:000, filepos: {:09x}\n",offset+starts[0],offset+starts[1]));
    std::fs::write(&idx, &index).unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", &fixture("av_long.mkv")]);
    h.wait("first-frame");
    h.drop_files(&[idx.to_str().unwrap()]);
    h.state_until("Japanese external VobSub text", |s| {
        s["subtitles"]["source"]
            .as_str()
            .is_some_and(|v| v.ends_with("external.IDX"))
            && s["subtitles"]["cues"] == 2
            && s["subtitles"]["incomplete"] == false
    });
    h.cmd(json!({"cmd":"click_grapheme","layer":"main","cue_index":0,"grapheme_index":3}));
    h.state_until("a word of the paired subtitle", |s| {
        s["popups"][0]["query"] == "食べました"
    });
    h.key("Escape");
    h.drop_files(&[sub.to_str().unwrap()]);
    h.state_until("opening the other member uses the same pair", |s| {
        s["subtitles"]["source"]
            .as_str()
            .is_some_and(|v| v.ends_with("external.SUB"))
            && s["subtitles"]["cues"] == 2
            && s["subtitles"]["incomplete"] == false
    });
    h.open(&fixture("av_noaudio.mp4"));
    h.wait("file-loaded");
    h.open(&fixture("av_long.mkv"));
    h.wait("file-loaded");
    h.state_until("saved explicit pair is restored on reopening", |s| {
        s["subtitles"]["source"]
            .as_str()
            .is_some_and(|v| v.ends_with("external.SUB"))
            && s["subtitles"]["cues"] == 2
            && s["subtitles"]["incomplete"] == false
    });
    let (single, starts) =
        dvd::program_stream_track(&[(0, dvd::spu(0, 0, &block(180, 20), 1000))], 2000, 3);
    std::fs::write(&sub, &single).unwrap();
    h.drop_files(&[idx.to_str().unwrap()]);
    h.state_until("changed invalid companion rejects the old cache", |s| {
        s["subtitles"]["cues"] == 0 && s["subtitles"]["incomplete"] == false
    });
    let index =
        dvd::idx(&dvd::PALETTE, &[5000], &starts).replace("id: ja, index: 0", "id: ja, index: 3");
    std::fs::write(&idx, index).unwrap();
    h.drop_files(&[idx.to_str().unwrap()]);
    h.state_until("changed valid pair counts its new single cue", |s| {
        s["subtitles"]["cues"] == 1 && s["subtitles"]["incomplete"] == false
    });
    std::fs::remove_file(&sub).unwrap();
    h.drop_files(&[idx.to_str().unwrap()]);
    h.state_until("missing companion fails without a partial result", |s| {
        s["subtitles"]["cues"] == 0 && s["subtitles"]["incomplete"] == false
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let files = std::fs::read_dir(desk.path("cache/nikomiru/subtitles"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("vob-"))
        .count();
    assert_eq!(
        files, 2,
        "only the two complete valid pair versions are cached"
    );
}
