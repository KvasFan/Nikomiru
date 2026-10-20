// SPDX-License-Identifier: GPL-3.0-only

//! L4: the `anki` worker against the fake AnkiConnect, with real dictionaries
//! and a real ffmpeg, a fake frame instead of mpv (T-ANK-003, T-ANK-030,
//! T-ANK-035…T-ANK-038).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use nikomiru_anki::fake::{Behaviour, FakeAnki};
use nikomiru_anki::{Client, lapis};
use nikomiru_app::anki::{Anki, Frames, Parts};
use nikomiru_core::config::AnkiConfig;
use nikomiru_core::{Cancel, ManualClock};
use nikomiru_library::{DictionaryManager, Library};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::mining::{
    AnkiService, BrowseTarget, CardAction, CheckRequest, MineFailure, MineRequest, Mined,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

/// A frame drawn by ffmpeg, as mpv would save it.
struct FakeFrames;

impl Frames for FakeFrames {
    fn save(&self, path: &Path) -> Result<(), String> {
        let status = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("color=c=red:s=320x180")
            .args(["-frames:v", "1"])
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| "ffmpeg".into())
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    fake: FakeAnki,
    anki: Anki,
    clock: Arc<ManualClock>,
    library: Arc<Library>,
}

fn setup(config: impl FnOnce(&mut AnkiConfig)) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeAnki::start().unwrap();
    lapis::create(&Client::new(&fake.url(), "").unwrap()).unwrap();
    fake.clear_requests();
    let library = Arc::new(Library::open(&dir.path().join("library.redb")).unwrap());
    let clock = Arc::new(ManualClock::new(SystemTime::UNIX_EPOCH));
    let manager = DictionaryManager::open(
        Arc::clone(&library),
        dir.path().join("dicts"),
        clock.clone(),
        "ja",
    )
    .unwrap();
    for name in ["mini-ja", "mini-freq"] {
        manager
            .import(
                &fixture(&format!("dicts/{name}.zip")),
                None,
                &Cancel::new(),
                |_| {},
            )
            .unwrap();
    }
    let mut anki = AnkiConfig {
        url: fake.url(),
        ..AnkiConfig::default()
    };
    config(&mut anki);
    let service = Anki::start(Parts {
        config: anki,
        ffmpeg: String::new(),
        translator: Translator::from_setting("en", |_| None),
        dictionaries: Some(Arc::new(manager)),
        max_results: 10,
        frames: Box::new(FakeFrames),
        cache_dir: dir.path().join("cache/mining"),
        library: Some(Arc::clone(&library)),
        clock: clock.clone(),
    });
    Setup {
        _dir: dir,
        fake,
        anki: service,
        clock,
        library,
    }
}

fn request(video: &str, audio: Option<i64>, action: CardAction) -> MineRequest {
    MineRequest {
        query: "食べました".into(),
        entry: 0,
        expression: "食べる".into(),
        reading: "たべる".into(),
        matched: "食べました".into(),
        sentence: "食べました".into(),
        offset: Some(0),
        video: fixture(video),
        audio_stream: audio,
        clip: Some((1_880, 2_700)),
        position_ms: 2_000,
        uid: Some("video/av_basic".into()),
        title: "av_basic".into(),
        action,
    }
}

fn mine(s: &Setup, r: MineRequest) -> Result<Mined, MineFailure> {
    let (tx, rx) = mpsc::channel();
    s.anki
        .mine(r, Box::new(move |result| tx.send(result).unwrap()));
    rx.recv_timeout(Duration::from_secs(60)).unwrap()
}

fn check(s: &Setup, words: &[(&str, &str)]) -> Option<Vec<bool>> {
    let (tx, rx) = mpsc::channel();
    let entries = words
        .iter()
        .map(|(e, r)| ((*e).to_string(), (*r).to_string()))
        .collect();
    s.anki.check(
        CheckRequest { entries },
        Box::new(move |r| tx.send(r).unwrap()),
    );
    rx.recv_timeout(Duration::from_secs(30)).unwrap()
}

// T-ANK-035
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn without_audio_there_is_no_card() {
    let s = setup(|_| {});
    let failure = mine(&s, request("av_noaudio.mp4", None, CardAction::Add)).unwrap_err();
    assert!(failure.message.contains("audio"), "{}", failure.message);
    assert!(!s.fake.actions().contains(&"addNote".to_string()));
    assert!(s.fake.collection().media.is_empty());
}

// T-ANK-036, T-ANK-039
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn a_refused_note_takes_back_only_its_new_media() {
    let s = setup(|_| {});
    let made = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    assert!(s.library.mined_card(made.note_id).unwrap().is_some());
    let media: Vec<String> = s.fake.collection().media.keys().cloned().collect();
    assert_eq!(media.len(), 2, "{media:?}");
    // the same card again: a duplicate; the media were there, so they stay
    let failure = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap_err();
    assert!(failure.duplicate, "{}", failure.message);
    assert_eq!(
        s.fake
            .collection()
            .media
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        media
    );
    // another track: new audio, refused by Anki, deleted again
    s.fake.clear_requests();
    let failure = mine(&s, request("av_basic.mkv", Some(2), CardAction::Add)).unwrap_err();
    assert!(failure.duplicate);
    let actions = s.fake.actions();
    assert_eq!(
        actions.iter().filter(|a| *a == "deleteMediaFile").count(),
        1,
        "{actions:?}"
    );
    assert_eq!(s.fake.collection().media.len(), 2);
}

// T-ANK-003
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_unknown_outcome_keeps_the_media() {
    let s = setup(|_| {});
    // Anki adds the note, the reply is lost
    s.fake.on("addNote", Behaviour::ApplyThenClose);
    let failure = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap_err();
    assert!(
        failure.message.contains("did not confirm"),
        "{}",
        failure.message
    );
    let actions = s.fake.actions();
    assert_eq!(
        actions.iter().filter(|a| *a == "addNote").count(),
        1,
        "never twice"
    );
    assert!(
        !actions.contains(&"deleteMediaFile".to_string()),
        "{actions:?}"
    );
    let anki = s.fake.collection();
    assert_eq!((anki.notes.len(), anki.media.len()), (1, 2));
}

// T-ANK-037
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_overwrite_keeps_the_note_and_refuses_another_note_type() {
    let s = setup(|_| {});
    let made = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    let again = mine(
        &s,
        request("av_basic.mkv", Some(2), CardAction::Overwrite(made.note_id)),
    )
    .unwrap();
    assert_eq!((again.note_id, again.overwritten), (made.note_id, true));
    {
        let anki = s.fake.collection();
        assert_eq!(anki.notes.len(), 1);
        let hint = anki.notes[0]
            .fields
            .iter()
            .find(|(n, _)| n == "Hint")
            .unwrap();
        assert_eq!(hint.1, "", "unmapped fields are emptied");
    }
    // a note of another type is not overwritten
    let other = Client::new(&s.fake.url(), "")
        .unwrap()
        .add_note(
            &nikomiru_anki::Note {
                deck: "Default".into(),
                model: "Basic".into(),
                fields: vec![("Front".into(), "x".into())],
                tags: Vec::new(),
                scope: nikomiru_anki::DuplicateScope::Deck,
            },
            false,
        )
        .unwrap();
    let failure = mine(
        &s,
        request("av_basic.mkv", Some(1), CardAction::Overwrite(other)),
    )
    .unwrap_err();
    assert!(failure.message.contains("Basic"), "{}", failure.message);
}

// T-ANK-030, T-ANK-038
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn checks_find_cards_pause_after_a_failure_and_open_in_anki() {
    let s = setup(|_| {});
    assert_eq!(check(&s, &[("食べる", "たべる")]), Some(vec![false]));
    let made = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    assert_eq!(
        check(&s, &[("食べる", "たべる"), ("猫", "ねこ")]),
        Some(vec![true, false])
    );
    // a transport error: no checks for 30 s
    s.fake.then(Behaviour::Status(500));
    assert_eq!(check(&s, &[("食べる", "たべる")]), None);
    s.fake.clear_requests();
    assert_eq!(check(&s, &[("食べる", "たべる")]), None);
    assert!(s.fake.requests().is_empty(), "paused");
    s.clock.advance(Duration::from_secs(31));
    assert_eq!(check(&s, &[("食べる", "たべる")]), Some(vec![true]));
    // Open in Anki: the note, or the word through dupe:
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    s.anki.browse(
        BrowseTarget::Note(made.note_id),
        Box::new(move |r| tx.send(r).unwrap()),
    );
    rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    s.anki.browse(
        BrowseTarget::Word {
            expression: "食べる".into(),
            reading: "たべる".into(),
        },
        Box::new(move |r| tx2.send(r).unwrap()),
    );
    rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    let browsed = s.fake.collection().browsed.clone();
    assert_eq!(browsed[0], format!("nid:{}", made.note_id));
    assert!(browsed[1].starts_with("nid:"), "{browsed:?}");
    let queries = s.fake.requests();
    let find = queries.iter().find(|r| r["action"] == "findNotes").unwrap();
    assert!(
        find["params"]["query"].as_str().unwrap().contains("dupe:"),
        "{find}"
    );
}
