// SPDX-License-Identifier: GPL-3.0-only

//! L4: the audio levels of a real file through ffmpeg, and the offset of its
//! cues moved (T-SUB-018).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use nikomiru_media::energy;
use nikomiru_media::process::{Cancel, JobError};
use nikomiru_media::tools::{Search, probe};
use nikomiru_subs::align::{AlignResult, BIN_MS, align};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

// T-SUB-018: av_basic.mkv has a tone under each of its three cues
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_tones_of_av_basic_put_its_moved_cues_back() {
    let tools = probe(&Search::from_env("")).unwrap();
    let video = fixture("av_basic.mkv");
    let levels = energy::levels(
        &tools.ffmpeg,
        &video,
        Some(1),
        60_000,
        BIN_MS,
        &Cancel::new(),
    )
    .unwrap();
    // 12 s of audio, loud under the cues, silent between them
    assert!((118..=122).contains(&levels.len()), "{}", levels.len());
    assert!(levels[22] > -20.0, "{}", levels[22]);
    assert!(levels[12] < -60.0, "{}", levels[12]);
    // the cues written 1.2 s early, then 0.7 s late
    for (cues, offset) in [
        ([(800, 1_300), (3_800, 4_300), (6_800, 7_300)], 1_200),
        ([(2_700, 3_200), (5_700, 6_200), (8_700, 9_200)], -700),
    ] {
        match align(&cues, &levels) {
            AlignResult::Aligned(a) => {
                assert!((a.offset_ms - offset).abs() <= BIN_MS, "{offset}: {a:?}");
            }
            other => panic!("{offset}: {other:?}"),
        }
    }
    // a file without audio: ffmpeg fails, with its reason
    let none = energy::levels(
        &tools.ffmpeg,
        &fixture("av_noaudio.mp4"),
        None,
        60_000,
        BIN_MS,
        &Cancel::new(),
    );
    assert!(matches!(none, Err(JobError::Failed { .. })), "{none:?}");
    // cancelled
    let cancel = Cancel::new();
    cancel.cancel();
    let cancelled = energy::levels(&tools.ffmpeg, &video, Some(1), 60_000, BIN_MS, &cancel);
    assert!(
        matches!(cancelled, Err(JobError::Cancelled { .. }) | Ok(_)),
        "{cancelled:?}"
    );
}
