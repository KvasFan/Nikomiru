// SPDX-License-Identifier: GPL-3.0-only

//! L4: databases of the Local Audio Server written by SQLite itself
//! (`cargo xtask fixtures`, `xtask/scripts/android_db.py`) read by Nikomiru's
//! reader (T-AUD-003).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use nikomiru_dict::local_audio::{AudioFile, LocalAudio, LocalAudioError};

fn fixture(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures/audio")
        .join(name);
    assert!(
        path.exists(),
        "{} is missing: run `cargo xtask fixtures`",
        path.display()
    );
    path
}

fn names(files: &[AudioFile]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|f| (f.source.as_str(), f.file.as_str()))
        .collect()
}

/// The audio the script wrote for a file: its own name, repeated.
fn blob(file: &AudioFile, size: usize) -> Vec<u8> {
    let text = format!("{}|{}|", file.source, file.file).into_bytes();
    text.iter().copied().cycle().take(size).collect()
}

// T-AUD-003
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_exact_pair_then_the_expression_by_sub_source() {
    for db in ["android.db", "android-noindex.db"] {
        let audio = LocalAudio::open(&fixture(db)).unwrap();
        let files = audio.files("食べる", "たべる", &[]).unwrap();
        assert_eq!(
            names(&files),
            [
                ("nhk16", "nhk16/1.mp3"),
                ("forvo", "食べる/a.mp3"),
                ("forvo", "食べる/b.mp3"),
                ("jpod", "jpod/1.mp3"),
            ],
            "{db}"
        );
        // the priority of the settings
        let priority = vec!["jpod".to_string(), "forvo".to_string()];
        let files = audio.files("食べる", "たべる", &priority).unwrap();
        assert_eq!(files[0].source, "jpod", "{db}");
        assert_eq!(files[3].source, "nhk16", "{db}");
        // the pair, else the expression alone
        assert_eq!(
            names(&audio.files("行く", "いく", &[]).unwrap()),
            [("jpod", "jpod/2.mp3")]
        );
        assert_eq!(
            names(&audio.files("行く", "いこう", &[]).unwrap()),
            [("nhk16", "nhk16/2.mp3"), ("jpod", "jpod/2.mp3")],
            "{db}"
        );
        assert_eq!(audio.files("飲む", "", &[]).unwrap().len(), 1);
        assert!(audio.files("無い", "ない", &[]).unwrap().is_empty());
        // the audio, also on a chain of overflow pages (50 KB on 1 KiB pages)
        let nhk = &audio.files("食べる", "たべる", &[]).unwrap()[0];
        assert_eq!(
            audio.audio(nhk).unwrap().unwrap(),
            blob(nhk, 50_000),
            "{db}"
        );
        let shinmeikai = &audio.files("飲む", "のむ", &[]).unwrap()[0];
        assert_eq!(
            audio.audio(shinmeikai).unwrap().unwrap(),
            blob(shinmeikai, 3_000)
        );
        let filler = &audio.files("語1234", "ご1234", &[]).unwrap()[0];
        assert_eq!(audio.audio(filler).unwrap().unwrap(), blob(filler, 64));
        let missing = AudioFile {
            file: "x.mp3".into(),
            source: "forvo".into(),
        };
        assert_eq!(audio.audio(&missing).unwrap(), None);
    }
}

// T-AUD-003: what is not a database of audio is refused
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn other_files_are_refused() {
    for name in ["android-empty.db", "other.db"] {
        assert!(
            matches!(
                LocalAudio::open(&fixture(name)),
                Err(LocalAudioError::NotAudio)
            ),
            "{name}"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("notes.db");
    std::fs::write(&text, "not a database").unwrap();
    assert!(LocalAudio::open(&text).is_err());
    assert!(LocalAudio::open(&dir.path().join("missing.db")).is_err());
}

// T-AUD-003, T-SEC: a damaged file is an error, never a panic or a hang
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn damaged_files_give_errors() {
    let good = std::fs::read(fixture("android.db")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("damaged.db");
    // cut short
    for len in [100, 1024, 10_000, good.len() / 2] {
        std::fs::write(&path, &good[..len]).unwrap();
        if let Ok(audio) = LocalAudio::open(&path) {
            let _ = audio.files("食べる", "たべる", &[]);
        }
    }
    // bytes changed here and there
    let mut seed: u64 = 7;
    let mut random = move |n: usize| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (seed >> 33) as usize % n
    };
    let started = std::time::Instant::now();
    for _ in 0..300 {
        let mut bad = good.clone();
        for _ in 0..8 {
            let at = random(bad.len());
            bad[at] = random(256) as u8;
        }
        std::fs::write(&path, &bad).unwrap();
        if let Ok(audio) = LocalAudio::open(&path) {
            for (e, r) in [("食べる", "たべる"), ("語0042", "ご0042"), ("無い", "")] {
                if let Ok(files) = audio.files(e, r, &[]) {
                    for f in files.iter().take(2) {
                        let _ = audio.audio(f);
                    }
                }
            }
        }
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

// T-AUD-003: the synthetic database of spike S3 (300k entries), when there
#[test]
#[ignore = "L4: needs the database of spike S3 in target/spike-s3"]
fn the_spike_database_is_read_quickly() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/spike-s3/android.db");
    let Ok(words) = std::fs::read_to_string(path.with_extension("db.words")) else {
        return;
    };
    let audio = LocalAudio::open(&path).unwrap();
    let mut times = Vec::new();
    let mut found = 0;
    for line in words.lines() {
        let Some((e, r)) = line.split_once('\t') else {
            continue;
        };
        let t = std::time::Instant::now();
        if let Some(f) = audio.files(e, r, &[]).unwrap().first() {
            assert!(audio.audio(f).unwrap().is_some());
            found += 1;
        }
        times.push(t.elapsed());
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    eprintln!("{} lookups, {found} found, p95 {p95:?}", times.len());
    assert!(p95 < std::time::Duration::from_millis(20), "{p95:?}");
}
