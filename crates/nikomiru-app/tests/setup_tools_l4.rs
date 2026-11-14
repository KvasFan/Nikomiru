// SPDX-License-Identifier: GPL-3.0-only

//! T-INS-007: selected executable installation against an isolated local host.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_app::{
    setup::Parts,
    setup_tools::{install, inventory, unpack},
};
use nikomiru_core::Cancel;
use nikomiru_online::{
    fake_files::{FakeFiles, zip64},
    http::Http,
    setup::{
        catalog::Catalog,
        tools::{Artifact, Tool},
    },
};
use nikomiru_ui::{i18n::Translator, setup::ToolPaths};
use sha2::{Digest, Sha256};
use std::path::Path;

fn parts(root: &Path, tool: Tool) -> Parts {
    Parts {
        proxy: String::new(),
        cache_dir: root.join("cache"),
        audio_dir: root.join("audio"),
        tools_dir: root.join("tools"),
        model_dir: root.join("model"),
        tool_catalog: vec![tool],
        manifest_urls: Vec::new(),
        translator: Translator::from_setting("en", |_| None),
        catalog: Catalog {
            version: 1,
            dictionaries: Vec::new(),
            audio: Vec::new(),
        },
    }
}

fn tool(host: &FakeFiles, bytes: &[u8]) -> Tool {
    Tool {
        id: "deno".into(),
        name: "Test Deno".into(),
        version: "test".into(),
        license: "MIT".into(),
        source: host.url("/source"),
        dependencies: Vec::new(),
        artifacts: vec![Artifact {
            platform: nikomiru_online::setup::tools::platform(),
            url: host.url("/tool"),
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            bytes: bytes.len() as u64,
            format: "raw".into(),
            // Deliberately absent from PATH; never modify process-wide environment.
            programs: vec!["nikomiru-test-deno".into()],
        }],
    }
}

// T-INS-007: hash, executable validation, persistence-ready paths, offline reuse and rollback.
#[test]
fn checked_private_installation_reuses_and_preserves_existing_files_on_failure() {
    let host = FakeFiles::start().unwrap();
    let script = b"#!/bin/sh\nprintf 'deno 2.9.8\\n'\n";
    host.put("/tool", script.to_vec());
    let tool = tool(&host, script);
    let root = tempfile::tempdir().unwrap();
    let parts = parts(root.path(), tool.clone());
    let settings = ToolPaths::default();
    assert!(
        inventory(&parts, &settings, &Cancel::new())[0]
            .path
            .is_none()
    );
    assert!(host.requests().is_empty(), "inventory is offline");
    let http = Http::new("").unwrap();
    let path = install(&parts, &http, &tool, &settings, &|_| {}, &Cancel::new()).unwrap();
    assert!(path.starts_with(&parts.tools_dir));
    assert_eq!(std::fs::read(&path).unwrap(), script);
    assert!(path.with_file_name("SOURCE.txt").is_file());
    host.clear_requests();
    assert_eq!(
        install(&parts, &http, &tool, &settings, &|_| {}, &Cancel::new()).unwrap(),
        path
    );
    assert!(
        host.requests().is_empty(),
        "validated installs are reused without download"
    );

    // A broken previous installation is retained if its replacement fails its hash.
    std::fs::write(&path, b"previous bytes").unwrap();
    host.put("/tool", b"corrupt replacement".to_vec());
    assert!(install(&parts, &http, &tool, &settings, &|_| {}, &Cancel::new()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous bytes");
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap().parent().unwrap())
            .unwrap()
            .count(),
        1
    );

    // A valid hash with an invalid executable is also refused before publishing.
    let invalid = b"#!/bin/sh\nprintf 'other program\\n'\n";
    host.put("/tool", invalid.to_vec());
    let invalid_tool = self::tool(&host, invalid);
    assert!(
        install(
            &parts,
            &http,
            &invalid_tool,
            &settings,
            &|_| {},
            &Cancel::new()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"previous bytes");
    host.clear_requests();
    let cancelled = Cancel::new();
    cancelled.cancel();
    assert!(install(&parts, &http, &tool, &settings, &|_| {}, &cancelled).is_err());
    assert!(host.requests().is_empty());

    host.put("/tool", script.to_vec());
    assert_eq!(
        install(&parts, &http, &tool, &settings, &|_| {}, &Cancel::new()).unwrap(),
        path
    );
    assert_eq!(std::fs::read(&path).unwrap(), script);
}

// T-INS-007: archive paths cannot escape staging, and duplicate/missing programs fail closed.
#[test]
fn archives_only_extract_required_regular_files() {
    let host = FakeFiles::start().unwrap();
    let mut artifact = tool(&host, b"x").artifacts.remove(0);
    artifact.format = "zip".into();
    artifact.programs = vec!["deno".into()];
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("download.zip");
    for (i, (entries, success)) in [
        (
            vec![
                ("bin/deno", b"good".as_slice()),
                ("README", b"ignored".as_slice()),
            ],
            true,
        ),
        (vec![("../deno", b"bad".as_slice())], false),
        (vec![("/deno", b"bad".as_slice())], false),
        (vec![("bin\\deno", b"bad".as_slice())], false),
        (
            vec![("deno", b"one".as_slice()), ("bin/deno", b"two".as_slice())],
            false,
        ),
        (vec![("README", b"missing".as_slice())], false),
    ]
    .into_iter()
    .enumerate()
    {
        let stage = root.path().join(format!("stage-{i}"));
        std::fs::create_dir(&stage).unwrap();
        std::fs::write(&file, zip64(&entries)).unwrap();
        assert_eq!(
            unpack(&file, &artifact, &stage, &Cancel::new()).is_ok(),
            success,
            "{entries:?}"
        );
        if success {
            assert_eq!(std::fs::read_dir(stage).unwrap().count(), 1);
        }
    }
    assert!(!root.path().join("deno").exists());
}

// T-INS-007: invalidate cached FFmpeg discovery after the assistant supplies a new path.
#[test]
fn active_thumbnail_worker_uses_the_new_ffmpeg_without_restart() {
    use nikomiru_app::thumbnails::ThumbnailWorker;
    use nikomiru_ui::thumbnails::ThumbnailService;
    use std::{os::unix::fs::PermissionsExt, sync::mpsc, time::Duration};
    let root = tempfile::tempdir().unwrap();
    let worker = ThumbnailWorker::start(
        root.path().join("missing").display().to_string(),
        root.path().join("cache"),
    );
    let frame = || {
        let (tx, rx) = mpsc::channel();
        worker.frame(
            root.path().join("video"),
            0,
            Box::new(move |r| tx.send(r).unwrap()),
        );
        rx.recv_timeout(Duration::from_secs(10)).unwrap()
    };
    assert!(frame().is_err());
    let ffmpeg = root.path().join("ffmpeg");
    std::fs::write(&ffmpeg, "#!/bin/sh\ncase \"$*\" in\n *-version*) printf 'ffmpeg version 8.0\\n';;\n *-encoders*) printf ' V....D png fake\\n';;\n *) for last; do :; done; printf 'new frame' > \"${last#file:}\";;\nesac\n").unwrap();
    std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::copy(&ffmpeg, root.path().join("ffprobe")).unwrap();
    worker.configure_ffmpeg(ffmpeg.display().to_string());
    assert_eq!(frame().unwrap(), b"new frame");
}
