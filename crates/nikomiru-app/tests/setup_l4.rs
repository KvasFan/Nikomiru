// SPDX-License-Identifier: GPL-3.0-only

//! L4: the `setup` worker against a fake host (T-INS-002…T-INS-004): a
//! dictionary of the catalogue downloaded; Fushi's pack read by range, its
//! `fushi.db` naming the libraries, the Japanese one downloaded, checked and
//! opened; too little space refused before anything is fetched.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use nikomiru_app::setup::{Parts, Setup, labels_of};
use nikomiru_online::fake_files::{FakeFiles, zip64};
use nikomiru_online::setup::catalog::{AudioLibrary, Catalog, Dictionary};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::setup::{Plan, SetupEvent, SetupService};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

fn catalog(host: &FakeFiles) -> Catalog {
    Catalog {
        version: 1,
        dictionaries: vec![Dictionary {
            id: "mini".into(),
            name: "mini-ja".into(),
            kind: "term".into(),
            title_prefix: "mini-ja".into(),
            size_bytes: 1,
            license: String::new(),
            source: String::new(),
            url: host.url("/mini-ja.zip"),
            sha256: None,
            ticked: true,
        }],
        audio: vec![
            AudioLibrary {
                id: "fushi-ja".into(),
                name: "android.db".into(),
                kind: "japanese".into(),
                size_bytes: 1,
                ticked: true,
            },
            AudioLibrary {
                id: "fushi-en".into(),
                name: "android_english.db".into(),
                kind: "english".into(),
                size_bytes: 1,
                ticked: false,
            },
        ],
    }
}

/// Runs `plan` and gathers the events until it finishes.
fn run(setup: &Setup, plan: Plan) -> Vec<SetupEvent> {
    let (tx, rx) = mpsc::channel();
    let tx = Arc::new(Mutex::new(tx));
    setup.install(
        plan,
        Box::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    );
    let mut events = Vec::new();
    loop {
        let e = rx.recv_timeout(Duration::from_secs(60)).unwrap();
        let done = e == SetupEvent::Finished;
        events.push(e);
        if done {
            return events;
        }
    }
}

// T-INS-003: the labels of the pack's `fushi.db`, Windows paths and all
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_pack_names_its_libraries() {
    let labels = labels_of(&fixture("audio/fushi.db")).unwrap();
    assert_eq!(labels["local_audio_1782831652275.db"], "android.db");
    assert_eq!(labels["local_audio_1783304412795.db"], "android_english.db");
}

// T-INS-002, T-INS-003, T-INS-004: a dictionary and the Japanese library
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn a_dictionary_and_a_library_are_installed() {
    let host = FakeFiles::start().unwrap();
    host.put(
        "/mini-ja.zip",
        std::fs::read(fixture("dicts/mini-ja.zip")).unwrap(),
    );
    let japanese = std::fs::read(fixture("audio/android.db")).unwrap();
    let english = std::fs::read(fixture("audio/android-noindex.db")).unwrap();
    let fushi = std::fs::read(fixture("audio/fushi.db")).unwrap();
    let pack = zip64(&[
        ("fushi.db", &fushi),
        ("localAudio/local_audio_1783304412795.db", &english),
        ("localAudio/local_audio_1782831652275.db", &japanese),
    ]);
    let manifest = host.put_pack(&pack, 64 << 10);
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(Parts {
        proxy: String::new(),
        cache_dir: dir.path().join("cache"),
        audio_dir: dir.path().join("audio"),
        manifest_urls: vec![manifest],
        translator: Translator::from_setting("en", |_| None),
        catalog: catalog(&host),
    });
    let events = run(
        &setup,
        Plan {
            dictionaries: vec!["mini".into()],
            audio: vec!["fushi-ja".into()],
        },
    );
    let dictionary = events.iter().find_map(|e| match e {
        SetupEvent::Dictionary { id, path } if id == "mini" => Some(path.clone()),
        _ => None,
    });
    let dictionary = dictionary.unwrap_or_else(|| panic!("{events:?}"));
    assert_eq!(
        std::fs::read(&dictionary).unwrap(),
        std::fs::read(fixture("dicts/mini-ja.zip")).unwrap()
    );
    let library = events.iter().find_map(|e| match e {
        SetupEvent::Audio { id, path } if id == "fushi-ja" => Some(path.clone()),
        _ => None,
    });
    let library = library.unwrap_or_else(|| panic!("{events:?}"));
    assert_eq!(library, dir.path().join("audio/android.db"));
    assert_eq!(
        std::fs::read(&library).unwrap(),
        japanese,
        "the Japanese one"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SetupEvent::Progress { id, .. } if id == "fushi-ja"))
    );
    assert!(
        !dir.path().join("cache/fushi.db").exists(),
        "the labels' copy goes"
    );
}

// T-INS-004: a library that is not an audio database is refused
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn a_library_that_is_not_audio_is_refused() {
    let host = FakeFiles::start().unwrap();
    let fushi = std::fs::read(fixture("audio/fushi.db")).unwrap();
    let other = std::fs::read(fixture("audio/other.db")).unwrap();
    let pack = zip64(&[
        ("fushi.db", &fushi),
        ("localAudio/local_audio_1782831652275.db", &other),
    ]);
    let manifest = host.put_pack(&pack, 64 << 10);
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(Parts {
        proxy: String::new(),
        cache_dir: dir.path().join("cache"),
        audio_dir: dir.path().join("audio"),
        manifest_urls: vec![manifest],
        translator: Translator::from_setting("en", |_| None),
        catalog: catalog(&host),
    });
    let events = run(
        &setup,
        Plan {
            dictionaries: Vec::new(),
            audio: vec!["fushi-ja".into(), "fushi-en".into()],
        },
    );
    let failed: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            SetupEvent::Failed { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(failed, ["fushi-ja", "fushi-en"], "{events:?}");
    assert!(!dir.path().join("audio/android.db").exists());
}
