// SPDX-License-Identifier: GPL-3.0-only

//! Real DVB bitmap conversion through the existing extraction service.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_media::{
    extract::{self, Extractor, Stream},
    process::Cancel,
};
use nikomiru_subs::{pgs::Graphic, vobsub};
use std::{path::Path, process::Command, time::Duration};

// T-SUB-032: DVB uses an explicit bitmap encoder and matching palette, never
// text extraction or a copied DVB stream passed to the DVD parser.
#[test]
fn dvb_extraction_uses_a_distinct_cache_and_explicit_palette() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("video.mkv");
    std::fs::write(&source, b"video").unwrap();
    let extractor = Extractor {
        ffmpeg: "ffmpeg".into(),
        cache_dir: dir.path().join("cache"),
    };
    let dvb = Stream {
        ff_index: 2,
        codec: extract::DVB.into(),
    };
    let dvd = Stream {
        ff_index: 2,
        codec: extract::DVD.into(),
    };
    assert_ne!(
        extractor.cache_path(&source, &dvb).unwrap(),
        extractor.cache_path(&source, &dvd).unwrap()
    );
    let args = Extractor::args(&source, &[(&dvb, dir.path().join("out.sub"))]);
    assert!(args.windows(2).any(|w| w == ["-c:s", "dvdsub"]));
    assert!(
        args.windows(2)
            .any(|w| w == ["-palette", extract::DVB_PALETTE])
    );
    assert!(extract::is_readable_graphic(extract::DVB));
    assert!(!extract::is_readable_graphic("dvb_teletext"));
}

// T-SUB-032: actual DVD fixture -> DVB encoding -> DVD conversion -> pixels,
// with retained display timing and cancellation before extraction.
#[test]
#[ignore = "L4: needs ffmpeg"]
fn actual_dvb_bitmaps_convert_back_to_readable_dvd_pictures() {
    use nikomiru_subs::{pgs::testing::block, vobsub::testing as dvd};
    let dir = tempfile::tempdir().unwrap();
    let (bytes, starts) = dvd::program_stream(
        &[
            (1000, dvd::spu(20, 20, &block(100, 20), 1000)),
            (4000, dvd::spu(20, 20, &block(120, 20), 1000)),
        ],
        2000,
    );
    let idx = dir.path().join("in.idx");
    std::fs::write(&idx, dvd::idx(&dvd::PALETTE, &[1000, 4000], &starts)).unwrap();
    std::fs::write(idx.with_extension("sub"), bytes).unwrap();
    let dvb = dir.path().join("dvb.ts");
    let result = Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-copyts", "-i"])
        .arg(&idx)
        .args(["-map", "0:s:0", "-c:s", "dvbsub", "-f", "mpegts"])
        .arg(&dvb)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let extractor = Extractor {
        ffmpeg: Path::new("ffmpeg").into(),
        cache_dir: dir.path().join("cache"),
    };
    let stream = Stream {
        ff_index: 0,
        codec: extract::DVB.into(),
    };
    let outputs = extractor
        .extract(
            &dvb,
            std::slice::from_ref(&stream),
            Duration::from_secs(6),
            &Cancel::new(),
        )
        .unwrap();
    let data = std::fs::read(&outputs[0].1).unwrap();
    let palette = vobsub::palette(&format!("palette: {}", extract::DVB_PALETTE));
    let cues = vobsub::parse(&data, palette);
    assert_eq!(cues.len(), 2);
    assert!(cues.iter().all(|cue| {
        cue.end_ms > cue.start_ms
            && cue
                .picture()
                .is_some_and(|p| p.alpha.iter().any(|a| *a > 0))
    }));
    assert!((cues[1].start_ms - cues[0].start_ms - 3000).abs() < 100);
    let cancel = Cancel::new();
    cancel.cancel();
    let other = Extractor {
        ffmpeg: "ffmpeg".into(),
        cache_dir: dir.path().join("cancelled"),
    };
    assert!(
        other
            .extract(&dvb, &[stream], Duration::from_secs(6), &cancel)
            .is_err()
    );
    assert!(
        std::fs::read_dir(dir.path().join("cancelled"))
            .map_or(true, |d| d.flatten().all(|e| !e.path().is_file()))
    );
}
