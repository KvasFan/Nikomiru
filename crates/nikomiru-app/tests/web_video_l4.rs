// SPDX-License-Identifier: GPL-3.0-only

//! PLY-023: a fake yt-dlp, real local HTTP media and cancellable resolution.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_app::web_video::{Worker, job, resolve};
use nikomiru_core::Cancel;
use nikomiru_online::{
    fake_files::FakeFiles,
    http::{Http, checked_url},
};
use nikomiru_ui::{
    i18n::Translator,
    web_video::{Request, WebVideoService},
};
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, path::Path, sync::mpsc, time::Duration};

fn setup(root: &Path, host: &FakeFiles) -> Request {
    let script = root.join("yt-dlp");
    std::fs::write(&script, include_str!("support/fake_yt_dlp.py")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(script.with_extension("json"),json!({"title":"日本語の動画","description":"x".repeat(128*1024),"url":host.url("/video"),"protocol":"http","subtitles":{"ja":[{"url":host.url("/ja.vtt"),"ext":"vtt"}]}}).to_string()).unwrap();
    Request {
        generation: 1,
        page: host.url("/page"),
        executable: script.to_string_lossy().into(),
        proxy: String::new(),
        languages: vec!["ja".into()],
        folder: root.join("subs"),
        cancel: Cancel::new(),
    }
}

// T-PLY-023
#[test]
fn a_resolved_video_keeps_the_page_subtitles_and_media_behind_a_relay() {
    let dir = tempfile::tempdir().unwrap();
    let host = FakeFiles::start().unwrap();
    host.put("/video", b"media bytes".to_vec());
    host.put(
        "/ja.vtt",
        b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\ntext\n".to_vec(),
    );
    let r = setup(dir.path(), &host);
    let tr = Translator::from_setting("en", |_| None);
    assert!(host.requests().is_empty());
    let ready = resolve(&r, &tr).unwrap();
    assert_eq!(ready.page, r.page);
    assert_eq!(ready.title, "日本語の動画");
    assert_eq!(ready.uid, nikomiru_online::web_video::uid(&r.page));
    assert_eq!(ready.subtitles.len(), 1);
    assert_ne!(ready.video, host.url("/video"));
    let body = Http::new("")
        .unwrap()
        .get(&checked_url(&ready.video).unwrap(), &[])
        .unwrap()
        .body;
    assert_eq!(body, b"media bytes");
    let args: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("yt-dlp.args.json")).unwrap(),
    )
    .unwrap();
    for flag in [
        "--ignore-config",
        "--no-plugin-dirs",
        "--skip-download",
        "--write-subs",
        "--write-auto-subs",
    ] {
        assert!(args.iter().any(|a| a == flag));
    }
    assert!(args.iter().any(|a| a == "--print"));
    assert!(!args.iter().any(|a| a == "--dump-single-json"));
    assert_eq!(args.last().unwrap(), &r.page);
    assert!(host.requests().iter().any(|(p, _)| p == "/ja.vtt"));
}

// T-PLY-023
#[test]
fn missing_program_and_superseded_requests_do_not_start_unwanted_network_work() {
    let dir = tempfile::tempdir().unwrap();
    let host = FakeFiles::start().unwrap();
    host.put("/ja.vtt", b"WEBVTT\n".to_vec());
    let r = setup(dir.path(), &host);
    let tr = Translator::from_setting("en", |_| None);
    let mut missing = r.clone();
    missing.executable = dir.path().join("absent").to_string_lossy().into();
    assert!(resolve(&missing, &tr).unwrap_err().contains("yt-dlp"));
    assert!(host.requests().is_empty());
    let worker = Worker::start(tr);
    let mut slow = r.clone();
    slow.page = host.url("/slow");
    let (tx, rx) = mpsc::channel();
    worker.open(slow, Box::new(move |r| tx.send(r).unwrap()));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !dir.path().join("yt-dlp.args.json").exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut r = r;
    r.cancel = Cancel::new();
    r.generation = 2;
    let (tx2, rx2) = mpsc::channel();
    worker.open(r, Box::new(move |r| tx2.send(r).unwrap()));
    assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().is_err());
    assert!(rx2.recv_timeout(Duration::from_secs(3)).unwrap().is_ok());
}

// T-PLY-023
#[test]
fn url_and_proxy_are_arguments_and_the_resolver_has_a_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let host = FakeFiles::start().unwrap();
    let mut r = setup(dir.path(), &host);
    r.page = "https://example.org/watch?v=$(touch bad)&x=hello".into();
    r.proxy = "http://proxy.example:8080".into();
    let job = job(Path::new("/usr/bin/yt-dlp"), &r);
    assert_eq!(job.args.last().unwrap(), r.page.as_str());
    assert_eq!(job.timeout, Duration::from_secs(120));
    assert!(
        job.args
            .windows(2)
            .any(|p| p[0] == "--proxy" && p[1] == r.proxy.as_str())
    );
}
