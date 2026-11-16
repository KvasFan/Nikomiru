// SPDX-License-Identifier: GPL-3.0-only

//! M119: signed updates checked, downloaded with resumption and installed
//! into a portable test root, against a fake feed signed with a test key.
use super::support::{Desktop, exit_status};
use nikomiru_online::fake_files::FakeFiles;
use nikomiru_online::updates::test_key;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::time::Duration;

fn release_zip(says: &str) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let exec = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    zip.start_file("nikomiru", exec).unwrap();
    zip.write_all(format!("#!/bin/sh\necho 'nikomiru {says}'\n").as_bytes())
        .unwrap();
    zip.start_file(
        "NOTES.txt",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(&vec![b'n'; 200_000]).unwrap();
    zip.finish().unwrap().into_inner()
}

fn publish(host: &FakeFiles, channel: &str, release: &str, archive: &[u8], seed: u8) {
    let sha: String = Sha256::digest(archive)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let path = format!("/rel/{release}.zip");
    host.put(&path, archive.to_vec());
    let manifest = serde_json::to_vec(&json!({
        "format": "nikomiru-update", "version": 1, "channel": channel, "release": release,
        "published": "2026-11-16", "notes_url": host.url("/notes"),
        "artifacts": [{"platform": nikomiru_online::setup::tools::platform(), "kind": "zip",
            "url": host.url(&path), "sha256": sha, "bytes": archive.len()}],
    }))
    .unwrap();
    host.put(&format!("/feed/{channel}.json"), manifest.clone());
    host.put(
        &format!("/feed/{channel}.json.minisig"),
        test_key::sign(seed, &manifest).into_bytes(),
    );
}

// T-UPD-001…T-UPD-004: explicit check, a tampered feed refused, an interrupted
// download resumed, a release that is not what it says refused, the install
// swapped in, and a replayed older manifest refused.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn signed_updates_install_into_a_portable_root() {
    let host = FakeFiles::start().unwrap();
    let mut desk = Desktop::start();
    let root = desk.path("portable/Nikomiru");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("old.txt"), "old").unwrap();
    desk.set_env("NIKOMIRU_TEST_UPDATE_KEY", &test_key::public(1));
    desk.set_env("NIKOMIRU_TEST_UPDATE_FEED", &host.url("/feed"));
    desk.set_env("NIKOMIRU_TEST_UPDATE_ROOT", &root.to_string_lossy());
    // signed by another key: refused
    let good = release_zip("9.9.9");
    publish(&host, "stable", "9.9.9", &good, 2);
    let (mut app, mut h) = desk.launch();
    let s = h.state();
    assert_eq!(s["updates"]["configured"], true);
    assert_eq!(s["updates"]["owner"], "Portable");
    assert!(host.requests().is_empty(), "nothing is checked by itself");
    h.cmd(json!({"cmd":"updates","action":"check"}));
    h.state_until("wrong key refused", |s| {
        s["updates"]["checking"] == false && s["updates"]["error"].is_string()
    });
    // a release whose program is not what it says is not installed
    publish(&host, "stable", "9.9.9", &release_zip("1.0.0"), 1);
    h.cmd(json!({"cmd":"updates","action":"check"}));
    h.state_until("offered", |s| {
        s["updates"]["available"]["release"] == "9.9.9"
    });
    h.cmd(json!({"cmd":"updates","action":"download"}));
    h.state_until("downloaded", |s| s["updates"]["downloaded"].is_string());
    h.cmd(json!({"cmd":"updates","action":"install"}));
    h.state_until("refused install", |s| {
        s["updates"]["installing"] == false && s["updates"]["error"].is_string()
    });
    assert!(root.join("old.txt").exists(), "the install is untouched");
    // the real release: an interrupted download resumes
    publish(&host, "stable", "9.9.9", &good, 1);
    host.cut_after("/rel/9.9.9.zip", 50_000);
    h.cmd(json!({"cmd":"updates","action":"check"}));
    h.state_until("offered again", |s| {
        s["updates"]["available"]["release"] == "9.9.9" && s["updates"]["checking"] == false
    });
    h.cmd(json!({"cmd":"updates","action":"download"}));
    h.state_until("interrupted", |s| {
        s["updates"]["downloading"] == false && s["updates"]["error"].is_string()
    });
    host.clear_requests();
    h.cmd(json!({"cmd":"updates","action":"download"}));
    h.state_until("resumed", |s| s["updates"]["downloaded"].is_string());
    assert!(
        host.requests()
            .iter()
            .any(|(p, r)| p == "/rel/9.9.9.zip" && r.as_deref().is_some_and(|r| r != "bytes=0-"))
    );
    h.cmd(json!({"cmd":"updates","action":"install"}));
    h.state_until("installed", |s| s["updates"]["installed"] == "9.9.9");
    assert!(root.join("NOTES.txt").exists());
    assert!(!root.join("old.txt").exists());
    assert!(desk.path("portable/Nikomiru.previous/old.txt").exists());
    // a replayed older (yet newer than running) signed manifest is refused
    publish(&host, "stable", "9.0.0", &good, 1);
    h.cmd(json!({"cmd":"updates","action":"check"}));
    let replay = h.state_until("replay checked", |s| s["updates"]["checking"] == false);
    assert!(
        replay["updates"]["available"].is_null(),
        "{}",
        replay["updates"]
    );
    assert_eq!(replay["updates"]["up_to_date"], true);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
