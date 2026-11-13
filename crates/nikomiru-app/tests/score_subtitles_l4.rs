// SPDX-License-Identifier: GPL-3.0-only

//! KNW-008: embedded subtitle resolution, bounded caches and shutdown.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_app::{dictionaries::Dictionaries, score_subtitles::Resolver};
use nikomiru_core::{Cancel, ManualClock};
use nikomiru_library::{DictionaryManager, Library, SubtitleChoice};
use nikomiru_ui::{dictionaries::DictionaryService, i18n::Translator, scores::CountRequest};
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};

fn setup(root: &Path) -> CountRequest {
    for tool in ["ffmpeg", "ffprobe"] {
        let path = root.join(tool);
        std::fs::write(&path, include_str!("support/fake_score_tools.py")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(
        root.join("metadata.json"),
        r#"{"streams":[
        {"index":3,"codec_name":"subrip","tags":{"language":"jpn","title":"SRT"}},
        {"index":4,"codec_name":"ass","tags":{"language":"jpn","title":"ASS"}},
        {"index":5,"codec_name":"subrip","tags":{"language":"eng","title":"English"}},
        {"index":6,"codec_name":"hdmv_pgs_subtitle","tags":{"language":"jpn","title":"PGS"}}
    ],"format":{"duration":"12.5"}}"#,
    )
    .unwrap();
    for index in [3, 4, 5] {
        std::fs::write(
            root.join(format!("stream-{index}.txt")),
            format!("stream {index}"),
        )
        .unwrap();
    }
    let video = root.join("video with spaces.mkv");
    std::fs::write(&video, b"video").unwrap();
    CountRequest {
        video,
        saved: None,
        preferred: vec!["ja".into(), "en".into()],
        subtitles_dir: root.join("downloaded"),
        ffmpeg: root.to_string_lossy().into(),
        generation: 0,
        epoch: 0,
    }
}

fn tr() -> Translator {
    Translator::from_setting("en", |_| None)
}

fn calls(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} did not appear",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

// T-KNW-008: cached probing/extraction and source identity follow the video.
#[test]
fn embedded_cache_is_reused_and_invalidated_for_a_changed_video_or_track() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut request = setup(root);
    let cache = root.join("cache");
    let mut resolver = Resolver::new(cache);
    let cancel = Cancel::new();
    let first = resolver.resolve(&request, &cancel, &tr()).unwrap().unwrap();
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "stream 3");
    assert_eq!(
        resolver.resolve(&request, &cancel, &tr()).unwrap(),
        Some(first.clone())
    );
    assert_eq!(calls(root), ["probe", "extract"]);
    request.saved = Some(SubtitleChoice::Embedded {
        ff_index: Some(4),
        language: Some("jpn".into()),
        title: Some("ASS".into()),
    });
    let second = resolver.resolve(&request, &cancel, &tr()).unwrap().unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "stream 4");
    assert_eq!(calls(root), ["probe", "extract", "extract"]);
    std::fs::write(&request.video, b"a changed video").unwrap();
    let changed = resolver.resolve(&request, &cancel, &tr()).unwrap().unwrap();
    assert_ne!(second, changed);
    assert_eq!(
        calls(root),
        ["probe", "extract", "extract", "probe", "extract"]
    );
}

// T-KNW-008: decisive files/off avoid tools; image subtitles never start OCR.
#[test]
fn files_off_remote_and_graphic_sources_do_not_launch_unneeded_work() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut request = setup(root);
    let mut resolver = Resolver::new(root.join("cache"));
    let cancel = Cancel::new();
    request.saved = Some(SubtitleChoice::Off);
    assert!(
        resolver
            .resolve(&request, &cancel, &tr())
            .unwrap()
            .is_none()
    );
    request.saved = None;
    let file = root.join("video with spaces.ja.srt");
    std::fs::write(&file, "external").unwrap();
    assert_eq!(
        resolver.resolve(&request, &cancel, &tr()).unwrap(),
        Some(file)
    );
    assert!(calls(root).is_empty());
    request.saved = Some(SubtitleChoice::Embedded {
        ff_index: Some(6),
        language: Some("jpn".into()),
        title: Some("PGS".into()),
    });
    assert!(
        resolver
            .resolve(&request, &cancel, &tr())
            .unwrap()
            .is_none()
    );
    assert_eq!(calls(root), ["probe"]);
    request.video = "https://example.invalid/video.mkv".into();
    assert!(
        resolver
            .resolve(&request, &cancel, &tr())
            .unwrap()
            .is_none()
    );
    assert_eq!(calls(root), ["probe"]);
}

// T-KNW-008, T-EXT-003: active probe/extraction is cancelled, no partial file.
#[test]
fn probe_and_extraction_share_cancellation_and_remove_partial_outputs() {
    for kind in ["probe", "extract"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let request = setup(root);
        std::fs::write(root.join(format!("{kind}.wait")), "").unwrap();
        let cache = root.join("cache");
        let mut resolver = Resolver::new(cache.clone());
        let cancel = Cancel::new();
        let worker_cancel = cancel.clone();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            tx.send(resolver.resolve(&request, &worker_cancel, &tr()))
                .unwrap();
        });
        wait_for(&root.join(format!("{kind}.started")));
        cancel.cancel();
        let error = rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        worker.join().unwrap();
        assert!(
            std::fs::read_dir(cache)
                .map(|mut d| d.next().is_none())
                .unwrap_or(true)
        );
    }
}

// T-KNW-008: shutdown cancels an active child and drops queued episodes.
#[test]
fn dropping_the_dictionary_service_does_not_wait_for_a_probe_or_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let request = setup(root);
    std::fs::write(root.join("probe.wait"), "").unwrap();
    let library = Arc::new(Library::open(&root.join("library.redb")).unwrap());
    let manager = Arc::new(
        DictionaryManager::open(
            library,
            root.join("dicts"),
            Arc::new(ManualClock::new(SystemTime::UNIX_EPOCH)),
            "ja",
        )
        .unwrap(),
    );
    let service = Dictionaries::start(
        manager,
        tr(),
        Arc::new(AtomicUsize::new(32)),
        root.join("cache"),
    );
    let (tx, rx) = mpsc::channel();
    service.count_words(
        request.clone(),
        Box::new(move |done| {
            tx.send(done).unwrap();
        }),
    );
    wait_for(&root.join("probe.started"));
    let queued_callbacks = Arc::new(AtomicUsize::new(0));
    for _ in 0..4 {
        let queued_callbacks = Arc::clone(&queued_callbacks);
        service.count_words(
            request.clone(),
            Box::new(move |_| {
                queued_callbacks.fetch_add(1, Ordering::SeqCst);
            }),
        );
    }
    let before = Instant::now();
    drop(service);
    assert!(before.elapsed() < Duration::from_secs(2));
    assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap().is_err());
    assert_eq!(queued_callbacks.load(Ordering::SeqCst), 0);
    assert_eq!(calls(root), ["probe"]);
}

// T-KNW-008: real ffprobe/ffmpeg select, extract and parse SRT and ASS.
#[test]
#[ignore = "L4: needs ffmpeg and cargo xtask fixtures"]
fn real_embedded_text_tracks_are_resolved_and_parsed() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = CountRequest {
        video: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fixtures/av_basic.mkv"),
        saved: None,
        preferred: vec!["ja".into()],
        subtitles_dir: dir.path().join("subs"),
        ffmpeg: String::new(),
        generation: 0,
        epoch: 0,
    };
    let mut resolver = Resolver::new(dir.path().join("cache"));
    for saved in [
        None,
        Some(SubtitleChoice::Embedded {
            ff_index: None,
            language: Some("jpn".into()),
            title: Some("ASS".into()),
        }),
    ] {
        request.saved = saved;
        let file = resolver
            .resolve(&request, &Cancel::new(), &tr())
            .unwrap()
            .unwrap();
        let loaded = nikomiru_app::subtitles::read_file(&file, None, &tr()).unwrap();
        assert_eq!(loaded.track.cues.len(), 3);
        assert_eq!(loaded.track.cues[0].plain_text, "食べました");
        assert_eq!(
            file.extension().and_then(|s| s.to_str()),
            Some(if request.saved.is_some() {
                "ass"
            } else {
                "srt"
            })
        );
    }
}
