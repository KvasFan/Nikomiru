// SPDX-License-Identifier: GPL-3.0-only

//! L4: timeouts and cancellation of external processes, and the extraction of
//! embedded subtitles from `av_basic.mkv` (EXT-003, SUB-007).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nikomiru_media::extract::{Extractor, Stream};
use nikomiru_media::process::{Cancel, Job, JobError, run};
use nikomiru_media::tools::{Search, probe};
use nikomiru_subs::{Format, parse};

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A fake ffmpeg that starts writing its output, then hangs.
fn sleeper(dir: &Path) -> (Job, PathBuf) {
    let out = dir.join("partial.srt");
    let program = script(dir, "ffmpeg", "echo partial > \"$1\"; exec sleep 120");
    let mut job = Job::new(program, [out.as_os_str()]);
    job.outputs = vec![out.clone()];
    (job, out)
}

// T-EXT-003
#[test]
fn a_hanging_process_is_killed_at_its_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let (mut job, out) = sleeper(dir.path());
    job.timeout = Duration::from_millis(500);
    let started = Instant::now();
    let err = run(&job, &Cancel::new()).unwrap_err();
    assert!(matches!(err, JobError::Timeout { .. }), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!out.exists(), "the partial file is removed");
}

// T-EXT-003
#[test]
fn a_cancelled_process_ends_within_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let (job, out) = sleeper(dir.path());
    let cancel = Cancel::new();
    let canceller = cancel.clone();
    let started = Instant::now();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        canceller.cancel();
    });
    let err = run(&job, &cancel).unwrap_err();
    assert!(matches!(err, JobError::Cancelled { .. }), "{err}");
    assert!(started.elapsed() < Duration::from_millis(1300));
    assert!(!out.exists());
}

#[test]
fn a_failing_process_reports_its_error_output() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "fail", "echo 'bad input' >&2; exit 3");
    let err = run(&Job::new(program, [""; 0]), &Cancel::new()).unwrap_err();
    assert!(
        matches!(&err, JobError::Failed { stderr, .. } if stderr == "bad input"),
        "{err}"
    );
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

// T-SUB-007
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn embedded_tracks_in_one_pass_then_from_the_cache() {
    let real = probe(&Search {
        setting: String::new(),
        env: None,
        path: std::env::var_os("PATH"),
    })
    .unwrap()
    .ffmpeg;
    let dir = tempfile::tempdir().unwrap();
    let counter = dir.path().join("launches");
    let ffmpeg = script(
        dir.path(),
        "ffmpeg",
        &format!(
            "echo x >> '{}'\nexec '{}' \"$@\"",
            counter.display(),
            real.display()
        ),
    );
    let launches = || std::fs::read_to_string(&counter).map_or(0, |t| t.lines().count());
    let extractor = Extractor {
        ffmpeg,
        cache_dir: dir.path().join("cache"),
    };
    let streams = [
        Stream {
            ff_index: 3,
            codec: "subrip".into(),
        },
        Stream {
            ff_index: 4,
            codec: "ass".into(),
        },
    ];
    let video = fixture("av_basic.mkv");
    let files = extractor
        .extract(&video, &streams, Duration::from_secs(12), &Cancel::new())
        .unwrap();
    assert_eq!(launches(), 1, "both tracks in a single invocation");
    assert_eq!(files.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [3, 4]);
    let expected = ["食べました", "日本語を勉強しています。", "ラーメンが好き"];
    let srt = parse(Format::Srt, &std::fs::read_to_string(&files[0].1).unwrap());
    let srt_texts: Vec<&str> = srt.cues.iter().map(|c| c.plain_text.as_str()).collect();
    assert_eq!(srt_texts, expected);
    let ass = parse(Format::Ass, &std::fs::read_to_string(&files[1].1).unwrap());
    let ass_texts: Vec<&str> = ass.cues.iter().map(|c| c.plain_text.as_str()).collect();
    assert_eq!(
        ass_texts,
        ["食べました", "日本語を\n勉強しています。", "ラーメンが好き"]
    );
    assert_eq!(ass.cues[1].markup.anchor, Some(8));

    let again = extractor
        .extract(&video, &streams, Duration::from_secs(12), &Cancel::new())
        .unwrap();
    assert_eq!(again, files);
    assert_eq!(launches(), 1, "the second request comes from the cache");
}
