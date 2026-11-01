// SPDX-License-Identifier: GPL-3.0-only

//! L4: the `anki` worker against the fake AnkiConnect, with real dictionaries
//! and a real ffmpeg, a frame drawn by ffmpeg instead of mpv (T-ANK-003,
//! T-ANK-030, T-ANK-034…T-ANK-038).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use nikomiru_anki::fake::{Behaviour, FakeAnki};
use nikomiru_anki::{Client, lapis};
use nikomiru_app::anki::{Anki, Parts, STALE_MEDIA};
use nikomiru_core::config::AnkiConfig;
use nikomiru_core::{Cancel, ManualClock};
use nikomiru_library::{DictionaryManager, Library};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::mining::{
    AnkiService, BrowseTarget, CardAction, CheckRequest, FrameChoice, MineFailure, MineRequest,
    Mined, PendingFrame,
};
use serde_json::json;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

struct Setup {
    _dir: tempfile::TempDir,
    fake: FakeAnki,
    anki: Anki,
    clock: Arc<ManualClock>,
    library: Arc<Library>,
    /// The folder of the media.
    mining: PathBuf,
}

fn setup(config: impl FnOnce(&mut AnkiConfig)) -> Setup {
    setup_in(tempfile::tempdir().unwrap(), config)
}

fn setup_in(dir: tempfile::TempDir, config: impl FnOnce(&mut AnkiConfig)) -> Setup {
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
    let mining = dir.path().join("cache/mining");
    let service = Anki::start(Parts {
        config: anki,
        ffmpeg: String::new(),
        translator: Translator::from_setting("en", |_| None),
        dictionaries: Some(Arc::new(manager)),
        max_results: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(10)),
        cache_dir: mining.clone(),
        library: Some(Arc::clone(&library)),
        clock: clock.clone(),
    });
    Setup {
        _dir: dir,
        fake,
        anki: service,
        clock,
        library,
        mining,
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
        audio_input: fixture(video),
        audio_stream: audio,
        clip: Some((1_880, 2_700)),
        position_ms: 2_000,
        uid: Some("video/av_basic".into()),
        title: "av_basic".into(),
        action,
        frame: FrameChoice::Click,
        audio: true,
    }
}

/// The frame of the click, drawn by ffmpeg where the session has mpv save it.
fn frame(s: &Setup) -> PendingFrame {
    let path = s
        .mining
        .join(format!("{}.png", nikomiru_anki::link::new_source_id()));
    let target = path.clone();
    PendingFrame::new(
        path,
        Box::new(move |_| {
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            let status = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
                .arg("color=c=red:s=320x180")
                .args(["-frames:v", "1"])
                .arg(&target)
                .status()
                .map_err(|e| e.to_string())?;
            status
                .success()
                .then_some(())
                .ok_or_else(|| "ffmpeg".into())
        }),
    )
}

/// A frame the player could not save.
fn no_frame(s: &Setup) -> PendingFrame {
    PendingFrame::new(
        s.mining.join("never.png"),
        Box::new(|_| Err("the player stopped".into())),
    )
}

fn mine_with(s: &Setup, r: MineRequest, frame: Option<PendingFrame>) -> Result<Mined, MineFailure> {
    let (tx, rx) = mpsc::channel();
    s.anki
        .mine(r, frame, Box::new(move |result| tx.send(result).unwrap()));
    let result = rx.recv_timeout(Duration::from_secs(60)).unwrap();
    // nothing of the attempt is left behind (ADR 0007 §20)
    let left: Vec<PathBuf> = std::fs::read_dir(&s.mining)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
    result
}

fn mine(s: &Setup, r: MineRequest) -> Result<Mined, MineFailure> {
    mine_with(s, r, Some(frame(s)))
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

fn browse(s: &Setup, target: BrowseTarget) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    s.anki
        .browse(target, Box::new(move |r| tx.send(r).unwrap()));
    rx.recv_timeout(Duration::from_secs(30)).unwrap()
}

/// A field of the only note.
fn field(s: &Setup, name: &str) -> String {
    let anki = s.fake.collection();
    let note = anki.notes.last().unwrap();
    note.fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
        .unwrap()
}

// T-ANK-034
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_frame_of_the_click_is_the_picture_and_ffmpeg_stands_in_for_a_failed_one() {
    let s = setup(|_| {});
    mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    let picture = field(&s, "Picture");
    assert!(
        picture.starts_with(r#"<img src="nikomiru_"#) && picture.ends_with(r#".jpg">"#),
        "{picture}"
    );
    // the player could not give the frame: the same moment, from the file
    s.fake.collection().notes.clear();
    mine_with(
        &s,
        request("av_basic.mkv", Some(1), CardAction::Add),
        Some(no_frame(&s)),
    )
    .unwrap();
    assert!(
        field(&s, "Picture").starts_with(r#"<img src="nikomiru_"#),
        "{}",
        field(&s, "Picture")
    );
    // never asked of it: the card is still made, without a picture
    s.fake.collection().notes.clear();
    mine_with(&s, request("av_basic.mkv", Some(1), CardAction::Add), None).unwrap();
    assert_eq!(field(&s, "Picture"), "");
}

// T-ANK-040, T-ANK-041: the picture taken from the video at the start of
// the sentence, no audio, two lines; or no picture
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_advanced_card_has_the_picture_and_audio_chosen() {
    let s = setup(|_| {});
    let mut r = request("av_basic.mkv", Some(1), CardAction::Add);
    r.frame = FrameChoice::At(1_000);
    r.audio = false;
    r.sentence = "前の行\n食べました".into();
    r.offset = Some("前の行\n".len());
    // the frame of the click is not used: it goes too
    mine(&s, r).unwrap();
    let picture = field(&s, "Picture");
    assert!(picture.starts_with(r#"<img src="nikomiru_"#), "{picture}");
    assert_eq!(field(&s, "SentenceAudio"), "");
    assert_eq!(field(&s, "Sentence"), "前の行<br><b>食べました</b>");
    // a video without audio makes a card when its audio is left out
    s.fake.collection().notes.clear();
    let mut r = request("av_noaudio.mp4", None, CardAction::Add);
    r.audio = false;
    r.frame = FrameChoice::None;
    mine(&s, r).unwrap();
    assert_eq!(field(&s, "Picture"), "");
}

// T-ANK-035
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn without_audio_there_is_no_card() {
    let s = setup(|_| {});
    // no audio track
    let failure = mine(&s, request("av_noaudio.mp4", None, CardAction::Add)).unwrap_err();
    assert!(failure.message.contains("audio"), "{}", failure.message);
    // a track ffmpeg cannot cut
    let failure = mine(&s, request("av_noaudio.mp4", Some(1), CardAction::Add)).unwrap_err();
    assert!(failure.message.contains("audio"), "{}", failure.message);
    assert!(!s.fake.actions().contains(&"addNote".to_string()));
    assert!(s.fake.collection().media.is_empty());
    // a cue of no length gives no audio
    let mut empty = request("av_basic.mkv", Some(1), CardAction::Add);
    empty.clip = Some((2_000, 2_000));
    assert!(mine(&s, empty).is_err());
    assert!(s.fake.collection().notes.is_empty());
}

// T-ANK-033: an external audio track is cut from its own file
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_sentence_audio_comes_from_the_track_heard() {
    let s = setup(|_| {});
    let mut r = request("av_noaudio.mp4", Some(1), CardAction::Add);
    // av_basic.mkv stands for the external file: its stream 1 is audio
    r.audio_input = fixture("av_basic.mkv");
    mine(&s, r).unwrap();
    assert!(
        field(&s, "SentenceAudio").starts_with("[sound:nikomiru_"),
        "{}",
        field(&s, "SentenceAudio")
    );
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

// T-ANK-036: the name Anki gives a file is the one in the fields
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_fields_name_the_media_as_anki_stored_them() {
    let s = setup(|_| {});
    s.fake.on(
        "storeMediaFile",
        Behaviour::Json(json!({"result": "nikomiru_renamed.m4a", "error": null})),
    );
    mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    let requests = s.fake.requests();
    let first = requests
        .iter()
        .find(|r| r["action"] == "storeMediaFile")
        .unwrap();
    let asked = first["params"]["filename"].as_str().unwrap();
    let anki = s.fake.collection();
    let fields = &anki.notes[0].fields;
    assert!(
        fields
            .iter()
            .any(|(_, v)| v.contains("nikomiru_renamed.m4a")),
        "{fields:?}"
    );
    assert!(!fields.iter().any(|(_, v)| v.contains(asked)), "{asked}");
}

// T-ANK-003, T-ANK-030
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_unknown_outcome_keeps_the_media_and_pauses_the_checks() {
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
    {
        let anki = s.fake.collection();
        assert_eq!((anki.notes.len(), anki.media.len()), (1, 2));
    }
    // the checks wait 30 s
    s.fake.clear_requests();
    assert_eq!(check(&s, &[("食べる", "たべる")]), None);
    assert!(s.fake.requests().is_empty(), "paused");
    s.clock.advance(Duration::from_secs(31));
    assert_eq!(check(&s, &[("食べる", "たべる")]), Some(vec![true]));

    // a proxy's error, or a reply that is not AnkiConnect's: unknown as well
    for behaviour in [Behaviour::Status(502), Behaviour::Body("<html>".into())] {
        s.fake.collection().notes.clear();
        s.fake.clear_requests();
        s.fake.on("addNote", behaviour);
        let failure = mine(&s, request("av_basic.mkv", Some(2), CardAction::Add)).unwrap_err();
        assert!(
            failure.message.contains("did not confirm"),
            "{}",
            failure.message
        );
        assert!(!s.fake.actions().contains(&"deleteMediaFile".to_string()));
    }
    // a proxy that refuses: nothing reached Anki, the new media go
    s.fake.on("addNote", Behaviour::Status(403));
    s.fake.clear_requests();
    let mut other_clip = request("av_basic.mkv", Some(1), CardAction::Add);
    other_clip.clip = Some((1_000, 2_000));
    let failure = mine(&s, other_clip).unwrap_err();
    assert!(failure.message.contains("HTTP 403"), "{}", failure.message);
    assert_eq!(
        s.fake
            .actions()
            .iter()
            .filter(|a| *a == "deleteMediaFile")
            .count(),
        1
    );
}

// T-ANK-037
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_overwrite_writes_every_field_and_refuses_another_note_type() {
    let s = setup(|_| {});
    let made = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    let client = Client::new(&s.fake.url(), "").unwrap();
    client
        .update_note_fields(made.note_id, &[("Hint".into(), "old hint".into())])
        .unwrap();
    let audio = field(&s, "SentenceAudio");
    let again = mine(
        &s,
        request("av_basic.mkv", Some(2), CardAction::Overwrite(made.note_id)),
    )
    .unwrap();
    assert_eq!((again.note_id, again.overwritten), (made.note_id, true));
    assert_eq!(s.fake.collection().notes.len(), 1);
    assert_eq!(field(&s, "Hint"), "", "unmapped fields are emptied");
    assert_ne!(field(&s, "SentenceAudio"), audio, "the other track");
    assert_eq!(field(&s, "Expression"), "食べる");
    // a note of another type is not overwritten
    let other = client
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

// T-ANK-037: the media go back only while the note is as it was
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_overwrite_cut_short_keeps_the_media_the_note_may_use() {
    let s = setup(|_| {});
    let made = mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
    let overwrite = |track| {
        request(
            "av_basic.mkv",
            Some(track),
            CardAction::Overwrite(made.note_id),
        )
    };
    // Anki refuses the fields: the note is as it was, the new audio goes
    s.fake.on(
        "updateNoteFields",
        Behaviour::Json(
            json!({"result": null, "error": "close the affected editor and try again"}),
        ),
    );
    s.fake.clear_requests();
    let failure = mine(&s, overwrite(2)).unwrap_err();
    assert!(
        failure.message.contains("Close Anki"),
        "{}",
        failure.message
    );
    let actions = s.fake.actions();
    assert_eq!(
        actions.iter().filter(|a| *a == "deleteMediaFile").count(),
        1,
        "{actions:?}"
    );
    // the fields are written, the tags fail: the media stay
    s.fake.on("addTags", Behaviour::Status(503));
    s.fake.clear_requests();
    let failure = mine(&s, overwrite(2)).unwrap_err();
    assert!(
        failure.message.contains("did not confirm the overwrite"),
        "{}",
        failure.message
    );
    assert!(!s.fake.actions().contains(&"deleteMediaFile".to_string()));
    assert_eq!(s.fake.collection().media.len(), 3);
    // the reply to the fields is lost: they may be written, the media stay
    s.fake.collection().media.clear();
    s.fake.on("updateNoteFields", Behaviour::ApplyThenClose);
    s.fake.on("updateNoteFields", Behaviour::ApplyThenClose);
    s.fake.clear_requests();
    let failure = mine(&s, overwrite(1)).unwrap_err();
    assert!(
        failure.message.contains("did not confirm the overwrite"),
        "{}",
        failure.message
    );
    assert!(!s.fake.actions().contains(&"deleteMediaFile".to_string()));
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
    // any answer ends the pause: Open in Anki on the note
    browse(&s, BrowseTarget::Note(made.note_id)).unwrap();
    assert_eq!(check(&s, &[("食べる", "たべる")]), Some(vec![true]));
    // or 30 s
    s.fake.then(Behaviour::Status(500));
    assert_eq!(check(&s, &[("食べる", "たべる")]), None);
    s.clock.advance(Duration::from_secs(31));
    assert_eq!(check(&s, &[("食べる", "たべる")]), Some(vec![true]));
    // Open in Anki on the word: through dupe: for every note type, in the deck
    s.fake.clear_requests();
    browse(
        &s,
        BrowseTarget::Word {
            expression: "食べる".into(),
            reading: "たべる".into(),
        },
    )
    .unwrap();
    let browsed = s.fake.collection().browsed.clone();
    assert_eq!(browsed[0], format!("nid:{}", made.note_id));
    assert_eq!(browsed[1], format!("nid:{}", made.note_id));
    let queries = s.fake.requests();
    let find = queries.iter().find(|r| r["action"] == "findNotes").unwrap();
    let query = find["params"]["query"].as_str().unwrap();
    let models = s.fake.collection().models.len();
    assert_eq!(query.matches("\"dupe:").count(), models, "{query}");
    assert!(query.ends_with(r#" "deck:Lapis""#), "{query}");
}

// T-ANK-030, PRV-001: an AnkiConnect elsewhere is asked only on a click
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn an_anki_on_another_computer_is_never_checked() {
    // 0.0.0.0 is not a loopback address, yet Linux connects it to this one
    let s = setup(|c| c.url = c.url.replace("127.0.0.1", "0.0.0.0"));
    assert_eq!(check(&s, &[("食べる", "たべる")]), None);
    assert!(s.fake.requests().is_empty());
    // the click goes through
    mine(&s, request("av_basic.mkv", Some(1), CardAction::Add)).unwrap();
}

// ADR 0007 §20: what an attempt cut short left is removed at the next start
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn old_media_are_swept_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let mining = dir.path().join("cache/mining");
    std::fs::create_dir_all(&mining).unwrap();
    let (old, new) = (mining.join("old.png"), mining.join("new.png"));
    std::fs::write(&old, b"x").unwrap();
    std::fs::write(&new, b"x").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(SystemTime::now() - STALE_MEDIA - Duration::from_secs(60))
        .unwrap();
    let s = setup_in(dir, |_| {});
    // a job runs after the sweep
    check(&s, &[("食べる", "たべる")]);
    assert!(!old.exists());
    assert!(new.exists(), "it may be another window's");
}

// The jobs still queued when Nikomiru closes are not run
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn closing_drops_the_queued_jobs() {
    let Setup {
        _dir, fake, anki, ..
    } = setup(|_| {});
    let (entered, entered_rx) = mpsc::channel();
    let (gate, gate_rx) = mpsc::channel::<()>();
    let entries = vec![("食べる".to_string(), "たべる".to_string())];
    anki.check(
        CheckRequest {
            entries: entries.clone(),
        },
        Box::new(move |_| {
            entered.send(()).unwrap();
            let _ = gate_rx.recv();
        }),
    );
    entered_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let (ran, ran_rx) = mpsc::channel();
    for _ in 0..5 {
        let ran = ran.clone();
        anki.check(
            CheckRequest {
                entries: entries.clone(),
            },
            Box::new(move |_| ran.send(()).unwrap()),
        );
    }
    let closed = std::thread::spawn(move || {
        let start = Instant::now();
        drop(anki);
        start.elapsed()
    });
    // the job on the worker ends once the window is closing
    std::thread::sleep(Duration::from_millis(200));
    gate.send(()).unwrap();
    assert!(closed.join().unwrap() < Duration::from_secs(5));
    drop(ran);
    assert!(ran_rx.recv().is_err(), "a queued job ran");
    assert_eq!(
        fake.actions()
            .iter()
            .filter(|a| *a == "canAddNotesWithErrorDetail")
            .count(),
        1
    );
}
