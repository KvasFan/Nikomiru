// SPDX-License-Identifier: GPL-3.0-only

//! AUD-010: timeline planning and real Opus/MP3 exports, including chapters.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_media::{
    condense::{Format, Line, Plan, export},
    process::{Cancel, Job, file_arg, output, run},
    tools::{Search, find},
};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}
fn line(start_ms: i64, end_ms: i64, text: &str) -> Line {
    Line {
        start_ms,
        end_ms,
        text: text.into(),
    }
}

// T-AUD-010
#[test]
fn ranges_merge_without_losing_chapters_and_clip_to_the_media() {
    let plan = Plan::new(
        &[
            line(8000, 10_000, "last"),
            line(-120, 1000, "first"),
            line(1500, 2000, "nearby"),
            line(1500, 3000, "overlap"),
            line(12_000, 14_000, "outside"),
            line(4000, 4000, "empty"),
        ],
        9000,
    );
    assert_eq!(plan.spans, [(0, 3000), (8000, 9000)]);
    assert_eq!(plan.duration_ms(), 4000);
    assert_eq!(
        plan.chapters,
        [
            (0, "first".into()),
            (1500, "nearby / overlap".into()),
            (3000, "last".into())
        ]
    );
    assert!(Plan::new(&[line(0, 1000, "")], 5000).spans.is_empty());
    let separate = Plan::new(&[line(0, 1000, "a"), line(2000, 3000, "b")], 4000);
    assert_eq!(separate.spans.len(), 2, "exactly one second is removed");
}

// T-AUD-010
#[test]
#[ignore = "L4: needs ffmpeg and cargo xtask fixtures"]
fn exports_both_formats_with_exact_chapters_the_chosen_track_and_no_overwrite() {
    let (_, ffmpeg, ffprobe) = find(&Search::from_env("")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let plan = Plan::new(
        &[
            line(1880, 2700, "猫 = # ; \\ test"),
            line(4880, 5700, "犬"),
            line(7880, 8700, "三\n行"),
        ],
        12000,
    );
    for format in [Format::Opus, Format::Mp3] {
        let a = export(
            &ffmpeg,
            &fixture("av_basic.mkv"),
            Some(2),
            &plan,
            dir.path(),
            format,
            &Cancel::new(),
        )
        .unwrap();
        let before = std::fs::read(&a).unwrap();
        let b = export(
            &ffmpeg,
            &fixture("av_basic.mkv"),
            Some(2),
            &plan,
            dir.path(),
            format,
            &Cancel::new(),
        )
        .unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read(&a).unwrap(), before);
        let mut probe = Job::new(
            &ffprobe,
            [
                "-v",
                "error",
                "-show_chapters",
                "-show_format",
                "-of",
                "json",
            ],
        );
        probe.args.push(file_arg(&a));
        let value: serde_json::Value =
            serde_json::from_str(&output(&probe, &Cancel::new()).unwrap()).unwrap();
        let duration = value["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((duration - 2.46).abs() < 0.08, "{value}");
        let chapters = value["chapters"].as_array().unwrap();
        assert_eq!(chapters.len(), 3, "{value}");
        for (i, q) in chapters.iter().enumerate() {
            let start = q["start_time"].as_str().unwrap().parse::<f64>().unwrap();
            assert!((start - i as f64 * 0.82).abs() < 0.002, "{q}");
        }
        assert_eq!(chapters[0]["tags"]["title"], "猫 = # ; \\ test");
        assert_eq!(chapters[2]["tags"]["title"], "三 行");
        let pcm = dir.path().join("decoded.pcm");
        let mut decode = Job::new(&ffmpeg, ["-nostdin", "-v", "error", "-y", "-i"]);
        decode.args.push(file_arg(&a));
        decode
            .args
            .extend(["-ar", "16000", "-ac", "1", "-f", "s16le"].map(Into::into));
        decode.args.push(file_arg(&pcm));
        run(&decode, &Cancel::new()).unwrap();
        let samples: Vec<i16> = std::fs::read(&pcm)
            .unwrap()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let tone = &samples[4000..7200];
        let cycles = tone.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
        assert!(
            (cycles as f64 - 132.0).abs() < 4.0,
            "expected 660 Hz English track, got {cycles} cycles"
        );
        std::fs::remove_file(pcm).unwrap();
    }
    assert!(
        std::fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.'))
    );
}

// T-AUD-010
#[test]
#[ignore = "L4: needs ffmpeg and cargo xtask fixtures"]
fn cancellation_and_missing_audio_leave_no_partial_output() {
    let (_, ffmpeg, _) = find(&Search::from_env("")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let plan = Plan::new(&[line(0, 1000, "猫")], 5000);
    let cancel = Cancel::new();
    cancel.cancel();
    assert!(
        export(
            &ffmpeg,
            &fixture("av_basic.mkv"),
            None,
            &plan,
            dir.path(),
            Format::Opus,
            &cancel
        )
        .is_err()
    );
    assert!(
        export(
            &ffmpeg,
            &fixture("av_noaudio.mp4"),
            None,
            &plan,
            dir.path(),
            Format::Mp3,
            &Cancel::new()
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
