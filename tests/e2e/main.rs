// SPDX-License-Identifier: GPL-3.0-only

//! L6: the real `nikomiru` binary, built with the `test-harness` feature and
//! driven through the GUI test harness in an off-screen KWin (TEST §2).
//! `cargo xtask ci --full` runs them; by hand:
//! `cargo test -p nikomiru-app --features test-harness --test e2e -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod drag;
mod support;

use std::time::Duration;

use support::{Desktop, exit_status, fixture};

const NEEDS: &str = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`";

fn time_pos(state: &serde_json::Value) -> f64 {
    state["time_pos"].as_f64().unwrap_or(-1.0)
}

// T-E2E-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn open_play_seek_close_and_reopen_resumes() {
    let _ = NEEDS;
    let video = fixture("av_long.mkv");
    let mut desk = Desktop::start();

    let (mut app, mut h) = desk.launch();
    h.open(&video);
    h.wait("file-loaded");
    h.wait("first-frame");
    let playing = h.state_until("playback", |s| s["paused"] == false && time_pos(s) > 0.0);
    assert_eq!(playing["title"], "av_long.mkv");

    // seek forward from the keyboard: ArrowRight = +3 s by default (PLY-001)
    let before = time_pos(&h.state());
    h.key("ArrowRight");
    h.state_until("the seek", |s| time_pos(s) >= before + 2.5);

    // pause from the keyboard, then close the window
    h.key("Space");
    let paused = h.state_until("the pause", |s| s["paused"] == true);
    let saved = time_pos(&paused);
    assert!(saved > 3.0, "position {saved}");
    h.quit();
    let status = exit_status(&mut app, Duration::from_secs(15));
    assert!(status.success(), "{status}\n{}", desk.log());

    // reopen: the file resumes from the saved position (LIB-006, PLY-007)
    let (mut app, mut h) = desk.launch();
    h.open(&video);
    h.wait("file-loaded");
    let resumed = h.state_until("the resume position", |s| time_pos(s) > 0.0);
    let at = time_pos(&resumed);
    assert!(
        (saved - 0.5..saved + 2.0).contains(&at),
        "saved at {saved}, resumed at {at}\n{}",
        desk.log()
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-APP-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_second_launch_opens_its_file_in_the_first() {
    let video = fixture("av_basic.mkv");
    let mut desk = Desktop::start();
    let (mut first, mut h) = desk.launch();
    assert_eq!(h.state()["loaded"], false);

    let mut second = desk.app(&[&video]).spawn().unwrap();
    let status = exit_status(&mut second, Duration::from_secs(15));
    assert!(status.success(), "second launch: {status}\n{}", desk.log());

    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "av_basic.mkv");
    h.quit();
    assert!(exit_status(&mut first, Duration::from_secs(15)).success());
}

// T-LIB-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn files_open_from_the_command_line_and_by_drag_and_drop() {
    let basic = fixture("av_basic.mkv");
    let weird = fixture("-weird: name & spaces ü 日本.mkv");
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch_with(&["--", &basic]);
    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "av_basic.mkv");

    // a drop arrives as percent-encoded file:// URLs
    h.drop_files(&[&weird]);
    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "-weird: name & spaces ü 日本.mkv");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PLY-011
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn s_saves_the_frame_shown_in_the_pictures_folder() {
    let mut desk = Desktop::start();
    let pictures = desk.path("pictures");
    desk.set_env("XDG_PICTURES_DIR", &pictures.to_string_lossy());
    let (mut app, mut h) = desk.launch_with(&["--", &fixture("av_basic.mkv")]);
    h.wait("first-frame");
    h.key("KeyS");
    let saved = h.wait("screenshot-saved");
    let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert_eq!(path.parent(), Some(pictures.join("Nikomiru").as_path()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("av_basic ") && name.ends_with(".png"),
        "{name}"
    );
    let png = std::fs::read(&path).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let int = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    assert_eq!((int(16), int(20)), (1280, 720), "the video's own size");
    // T-KEY-005: S held down saves one picture, not one per repeat
    h.cmd(serde_json::json!({ "cmd": "key", "code": "KeyS", "repeat": 10 }));
    h.wait("screenshot-saved");
    std::thread::sleep(Duration::from_millis(1500));
    let files = std::fs::read_dir(pictures.join("Nikomiru"))
        .unwrap()
        .count();
    assert_eq!(files, 2);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-ANK-030, T-ANK-036, T-ANK-037, T-ANK-039, T-POP-014 (Ctrl+Enter), T-PERF-08
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_card_is_made_from_the_popup() {
    use nikomiru_anki::fake::FakeAnki;
    let fake = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&fake.url(), "").unwrap()).unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[anki]\nurl = \"{}\"\ntags = [\"mine\"]\n",
        fake.url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    // the check found no card: +
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");

    // Ctrl+Enter makes the card of the entry the keyboard is on
    let started = std::time::Instant::now();
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    let created = match h
        .try_cmd(serde_json::json!({ "cmd": "wait", "event": "card-created", "timeout_ms": 20000 }))
    {
        Ok(v) => v,
        Err(e) => panic!("{e}\nstate: {}\nlog:\n{}", h.state(), desk.log()),
    };
    let elapsed = started.elapsed();
    eprintln!("T-PERF-08 Ctrl+Enter → card-created: {elapsed:?}");
    assert!(elapsed <= Duration::from_secs(3), "T-PERF-08: {elapsed:?}");
    let note_id = created["note_id"].as_u64().unwrap();
    {
        let anki = fake.collection();
        let note = anki.notes.iter().find(|n| n.id == note_id).unwrap();
        assert_eq!(
            (note.deck.as_str(), note.model.as_str()),
            ("Lapis", "Lapis")
        );
        assert_eq!(note.tags, ["mine", "nikomiru", "video"]);
        let field = |name: &str| {
            note.fields
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        assert_eq!(field("Expression"), "食べる");
        assert_eq!(field("ExpressionFurigana"), "食[た]べる");
        assert_eq!(field("Sentence"), "<b>食べました</b>");
        assert_eq!(field("FreqSort"), "120");
        assert_eq!(field("IsWordAndSentenceCard"), "x");
        assert!(
            field("Glossary").contains("to eat"),
            "{}",
            field("Glossary")
        );
        let misc = field("MiscInfo");
        assert!(
            misc.contains("nikomiru://source?v=1&amp;kind=video&amp;uid=video%2Fav_basic&amp;ep=0&amp;startMs=1880&amp;endMs=2700"),
            "{misc}"
        );
        assert!(
            misc.ends_with(">av_basic</a> 00:00:01 - 00:00:02"),
            "{misc}"
        );
        let audio = field("SentenceAudio");
        let picture = field("Picture");
        assert!(
            audio.starts_with("[sound:nikomiru_") && audio.ends_with(".m4a]"),
            "{audio}"
        );
        assert!(
            picture.starts_with("<img src=\"nikomiru_") && picture.ends_with(".jpg\">"),
            "{picture}"
        );
        let names: Vec<&String> = anki.media.keys().collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let jpeg = anki
            .media
            .iter()
            .find(|(n, _)| n.ends_with(".jpg"))
            .unwrap()
            .1;
        assert_eq!(&jpeg[..3], [0xFF, 0xD8, 0xFF]);
    }
    h.state_until("the last card", |s| s["popups"][0]["cards"][0] == "last");

    // ✓↩: the menu, then overwrite (ANK-037): the same note
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0 }));
    h.state_until("the menu", |s| s["popups"][0]["menu"] == 0);
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0, "item": "overwrite" }));
    let again = h.wait("card-created");
    assert_eq!(again["note_id"].as_u64(), Some(note_id));
    assert_eq!(fake.collection().notes.len(), 1);
    assert!(fake.actions().contains(&"updateNoteFields".to_string()));
    // add anyway: a second note
    h.state_until("the last card", |s| s["popups"][0]["cards"][0] == "last");
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0, "item": "add-anyway" }));
    let second = h.wait("card-created")["note_id"].as_u64().unwrap();
    assert_ne!(second, note_id);
    assert_eq!(fake.collection().notes.len(), 2);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    // ANK-039: the cards made are in the library
    let library = nikomiru_library::Library::open(&desk.data_file("library.redb")).unwrap();
    let card = library.mined_card(second).unwrap().unwrap();
    assert_eq!(
        (card.expression.as_str(), card.video_uid.as_str()),
        ("食べる", "video/av_basic")
    );
    assert_eq!((card.start_ms, card.end_ms), (1880, 2700));
    assert_eq!(library.mined_card_count().unwrap(), 2);
}

// T-ANK-037: a card made after its popup closed is still the last card
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_card_finished_after_its_popup_closed_is_the_last_card() {
    use nikomiru_anki::fake::FakeAnki;
    let fake = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&fake.url(), "").unwrap()).unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[anki]\nurl = \"{}\"\n",
        fake.url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    // the card is asked for, the popup closes before Anki answers
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    h.key("Escape");
    h.state_until("no popup", |s| s["popups"] == serde_json::json!([]));
    let note_id = h.wait("card-created")["note_id"].as_u64().unwrap();
    // the word again: ✓↩ on that card
    h.cmd(serde_json::json!({ "cmd": "lookup", "text": "食べました" }));
    h.wait("popup-rendered");
    h.state_until("the last card", |s| s["popups"][0]["cards"][0] == "last");
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0 }));
    h.state_until("the menu", |s| s["popups"][0]["menu"] == 0);
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0, "item": "overwrite" }));
    assert_eq!(h.wait("card-created")["note_id"].as_u64(), Some(note_id));
    assert_eq!(fake.collection().notes.len(), 1);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-POP-014: a settings page over the popups takes their keys
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_page_over_the_popups_takes_their_keys() {
    use nikomiru_anki::fake::FakeAnki;
    let fake = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&fake.url(), "").unwrap()).unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[anki]\nurl = \"{}\"\n",
        fake.url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "open" }));
    h.state_until("the page", |s| s["anki_page"]["open"] == true);
    // Ctrl+Enter belongs to the popup under the page: no card
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    // Esc closes the page; the popup and its pause wait under it
    h.key("Escape");
    let s = h.state_until("the page closed", |s| s["anki_page"]["open"] == false);
    assert_eq!(s["popups"].as_array().map(Vec::len), Some(1), "{s}");
    assert_eq!(s["popups"][0]["cards"][0], "add", "{s}");
    assert_eq!(s["paused"], true, "{s}");
    assert!(!fake.actions().contains(&"addNote".to_string()));
    // the keys are the popup's again
    h.key("Escape");
    h.state_until("the popup closed", |s| s["popups"] == serde_json::json!([]));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-ANK-052, T-APP-001 with a link
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_card_link_reopens_the_scene_and_a_moved_video_is_located() {
    let mut desk = Desktop::start();
    let videos = desk.path("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let scene = videos.join("scene.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &scene).unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&scene.to_string_lossy());
    h.wait("file-loaded");
    // the second cue is [5.0, 5.5]; the link's clip is padded around it
    let link = "nikomiru://source?v=1&kind=video&uid=video%2Fscene&ep=0&startMs=4880&endMs=5700&sourceId=12345678-1234-4234-8234-123456789abc";
    let at_the_cue = |s: &serde_json::Value| {
        s["paused"] == true && (time_pos(s) - 5.0).abs() < 0.06 && s["loaded"] == true
    };

    // the video moved: the link offers to locate it
    let moved = videos.join("scene-moved.mkv");
    std::fs::rename(&scene, &moved).unwrap();
    h.cmd(serde_json::json!({ "cmd": "open", "path": link }));
    let s = h.state_until("the offer to locate", |s| !s["missing_link"].is_null());
    assert_eq!(s["missing_link"]["uid"], "video/scene");
    assert_eq!(s["missing_link"]["path"], scene.to_string_lossy().as_ref());
    h.cmd(serde_json::json!({ "cmd": "locate", "path": moved.to_string_lossy() }));
    h.wait("file-loaded");
    let s = h.state_until("at the cue, paused", at_the_cue);
    assert!(s["missing_link"].is_null());

    // from a second launch, as the desktop passes a link from Anki
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Space" }));
    h.state_until("playing", |s| s["paused"] == false);
    let mut second = desk.app(&[link]).spawn().unwrap();
    assert!(exit_status(&mut second, Duration::from_secs(15)).success());
    h.wait("file-loaded");
    h.state_until("at the cue again, paused", at_the_cue);

    // a link that is not one: a message, nothing opens
    h.cmd(serde_json::json!({ "cmd": "open", "path": "nikomiru://source?v=1&kind=video&uid=%2Fetc%2Fpasswd&ep=0&startMs=0&endMs=0&sourceId=12345678-1234-4234-8234-123456789abc" }));
    let s = h.state_until("the message", |s| {
        s["toast"]
            .as_str()
            .is_some_and(|t| t.contains("not a Nikomiru link"))
    });
    assert_eq!(s["title"], "scene-moved.mkv");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    // the library keeps the new place
    let library = nikomiru_library::Library::open(&desk.data_file("library.redb")).unwrap();
    let record = library.video("video/scene").unwrap().unwrap();
    assert_eq!(record.path, moved.to_string_lossy());
}

// T-ANK-006, T-ANK-011, T-SEC-008, SET-002 for Anki
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_anki_settings_test_create_lapis_and_save() {
    use std::os::unix::fs::PermissionsExt;
    let fake = nikomiru_anki::fake::FakeAnki::start().unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[anki]\nurl = \"{}\"\ndeck = \"Default\"\nnote_type = \"Basic\"\n",
        fake.url()
    ));
    let (mut app, mut h) = desk.launch();
    // the page reads what Anki has at once
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "open" }));
    let s = h.state_until("the lists", |s| !s["anki_page"]["models"].is_null());
    assert_eq!(s["anki_page"]["open"], true);
    assert_eq!(s["anki_page"]["models"], serde_json::json!(["Basic"]));
    assert_eq!(
        s["anki_page"]["fields"],
        serde_json::json!(["Front", "Back"])
    );
    // Create Lapis deck: the page then chooses Lapis and reads its fields
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "create-lapis" }));
    let s = h.state_until("Lapis", |s| {
        s["anki_page"]["fields"]
            .as_array()
            .is_some_and(|f| f.len() == 22)
    });
    assert_eq!(
        s["anki_page"]["models"],
        serde_json::json!(["Basic", "Lapis"])
    );
    // the choice of Lapis is saved (ANK-011)
    let file = desk.path("config/nikomiru/config.toml");
    let saved = support::until(Duration::from_secs(10), "the Lapis choice saved", || {
        let text = std::fs::read_to_string(&file).ok()?;
        let config = nikomiru_core::Config::from_toml(&text).ok()?;
        (config.anki.deck == "Lapis").then_some(config)
    });
    assert_eq!(saved.anki.note_type, "Lapis");
    assert_eq!(
        saved.anki.field_mapping["Lapis"]["Expression"],
        "{expression}"
    );
    // a deck Anki does not have is reported (ANK-006)
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "test", "config": format!("url = \"{}\"\ndeck = \"Nowhere\"", fake.url()) }));
    h.state_until("the missing deck", |s| {
        s["anki_page"]["status"]["text"]
            .as_str()
            .is_some_and(|t| t.contains("Nowhere"))
            && s["anki_page"]["status"]["ok"] == false
    });
    // Save: the file, readable by the user only
    let url = fake.url();
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "save", "config": format!(
        "url = \"{url}\"\ndeck = \"Lapis\"\nnote_type = \"Lapis\"\ntags = [\"from-settings\"]\naudio_pad_head_ms = 0\n"
    ) }));
    h.state_until("saved", |s| {
        s["anki_page"]["status"]["text"]
            .as_str()
            .is_some_and(|t| t.contains("saved"))
    });
    let text = std::fs::read_to_string(&file).unwrap();
    let config = nikomiru_core::Config::from_toml(&text).unwrap();
    assert_eq!(
        (config.anki.deck.as_str(), config.anki.note_type.as_str()),
        ("Lapis", "Lapis")
    );
    assert_eq!(config.anki.tags, ["from-settings"]);
    assert_eq!(config.player.mpv_options["ao"], "null", "the rest is kept");
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    // the next card follows the new settings
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "close" }));
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0 }));
    let id = h.wait("card-created")["note_id"].as_u64().unwrap();
    {
        let anki = fake.collection();
        let note = anki.notes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(note.deck, "Lapis");
        assert_eq!(note.tags, ["from-settings", "nikomiru", "video"]);
        let misc = &note.fields.iter().find(|(n, _)| n == "MiscInfo").unwrap().1;
        assert!(
            misc.contains("startMs=2000&amp;endMs=2700"),
            "no head pad: {misc}"
        );
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

/// Screenshots of the interface with the project's own media and test
/// dictionaries, for review and for the repository page (ROADMAP M4), into
/// the folder named by `NIKOMIRU_SCREENSHOTS`; without it, nothing.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon, spectacle and `cargo xtask fixtures`"]
fn screenshots_of_the_interface() {
    let Some(dir) = std::env::var_os("NIKOMIRU_SCREENSHOTS").map(std::path::PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let fake = nikomiru_anki::fake::FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&fake.url(), "").unwrap()).unwrap();
    let online = nikomiru_online::fake::FakeOnline::start().unwrap();
    {
        let mut d = online.data();
        d.entries = vec![(1, "av basic".into(), true)];
        d.files.insert(
            1,
            vec![
                ("av basic 01.ja.srt".into(), ONLINE_SRT.as_bytes().to_vec()),
                ("av basic 01.en.srt".into(), ONLINE_SRT.as_bytes().to_vec()),
                ("av basic S1.zip".into(), Vec::new()),
            ],
        );
    }
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\", deband = \"yes\", scale = \"ewa_lanczossharp\" }}\n\
         [anki]\nurl = \"{}\"\n[online]\njimaku_api_key = \"jimaku-key\"\njimaku_url = \"{}\"\n",
        fake.url(),
        online.jimaku_url()
    ));
    // av_basic's tracks over a calm gradient, made here: the test pattern
    // would take the eye off the interface
    let scene = desk.path("av basic.mkv");
    let status = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg("gradients=s=1280x720:d=12:r=25:c0=0x16213e:c1=0x3a2a5e:c2=0x0f3d4a:n=3:speed=0.004")
        .arg("-i")
        .arg(fixture("av_basic.mkv"))
        .args([
            "-map", "0:v", "-map", "1:a", "-map", "1:s", "-c:v", "libx264",
        ])
        .args([
            "-preset", "veryfast", "-pix_fmt", "yuv420p", "-c:a", "copy", "-c:s", "copy",
        ])
        .arg("-shortest")
        .arg(&scene)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg: {status}");
    // a library of links to the scene
    let root = desk.path("videos");
    for name in [
        "Frieren/Season 1/Frieren - 01.mkv",
        "Frieren/Season 1/Frieren - 02.mkv",
        "Frieren/Season 1/Frieren - 03.mkv",
        "Dandadan/Dandadan - 01.mkv",
        "Dandadan/Dandadan - 02.mkv",
        "Perfect Days (2023).mkv",
    ] {
        let link = root.join(name);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&scene, &link).unwrap();
    }
    let (mut app, mut h) = desk.launch();
    h.cmd(serde_json::json!({ "cmd": "library", "action": "import", "path": root }));
    h.state_until("the covers", |s| {
        s["library_page"]["covers"]
            .as_array()
            .is_some_and(|a| a.len() == 3)
    });
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("library.png"));
    h.open(&scene.to_string_lossy());
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("popup.png"));
    // the same popup in the light theme (SET-003)
    for (theme, dark) in [("light", false), ("dark", true)] {
        h.cmd(serde_json::json!({
            "cmd": "settings",
            "tab": "general",
            "save": format!("[general]\ntheme = \"{theme}\"\n"),
        }));
        h.state_until("the theme", |s| s["look"]["dark"] == dark);
        h.cmd(serde_json::json!({ "cmd": "settings", "tab": "close" }));
        if !dark {
            std::thread::sleep(Duration::from_millis(300));
            desk.capture(&dir.join("popup-light.png"));
        }
    }
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    h.wait("card-created");
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0 }));
    h.state_until("the menu", |s| s["popups"][0]["menu"] == 0);
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("popup-card-menu.png"));
    // the menu, then the popup; the rest is taken paused, so that the video
    // stays short of watched and in "Continue watching"
    h.key("Escape");
    h.state_until("the menu closed", |s| s["popups"][0]["menu"].is_null());
    h.key("Escape");
    h.state_until("the popup closed and playback resumed", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && s["paused"] == false
    });
    h.key("Space");
    h.state_until("paused", |s| s["paused"] == true);
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "open" }));
    h.state_until("the lists", |s| {
        s["anki_page"]["fields"]
            .as_array()
            .is_some_and(|f| f.len() == 22)
    });
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("anki-settings.png"));
    // the toast of the card gone
    std::thread::sleep(Duration::from_secs(2));
    for tab in [
        "general",
        "player",
        "subtitles",
        "services",
        "dictionaries",
        "lookup",
        "audio",
        "shortcuts",
        "advanced",
        "about",
    ] {
        h.cmd(serde_json::json!({ "cmd": "settings", "tab": tab }));
        h.state_until("the tab", |s| s["settings_tab"] == tab);
        std::thread::sleep(Duration::from_millis(300));
        desk.capture(&dir.join(format!("settings-{tab}.png")));
    }
    h.cmd(serde_json::json!({ "cmd": "settings", "tab": "close" }));
    h.cmd(serde_json::json!({ "cmd": "online", "action": "open" }));
    h.cmd(serde_json::json!({ "cmd": "online", "action": "search" }));
    h.state_until("the results", |s| {
        s["online_page"]["results"]["files"]
            .as_array()
            .is_some_and(|f| f.len() == 3)
    });
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("online-search.png"));
    h.cmd(serde_json::json!({ "cmd": "online", "action": "close" }));
    h.key("KeyL");
    h.state_until("the list", |s| s["sub_list_open"] == true);
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("subtitle-list.png"));
    h.cmd(serde_json::json!({ "cmd": "library", "action": "open" }));
    h.state_until("continue watching", |s| {
        s["library_page"]["continue"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    });
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("library-continue.png"));
    h.cmd(serde_json::json!({ "cmd": "library", "action": "statistics" }));
    h.state_until("the statistics", |s| s["library_page"]["stats"].is_object());
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("statistics.png"));
    // the light theme (SET-003)
    h.cmd(serde_json::json!({
        "cmd": "settings",
        "tab": "general",
        "save": "[general]\ntheme = \"light\"\n",
    }));
    h.state_until("the light theme", |s| s["look"]["dark"] == false);
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("settings-light.png"));
    h.cmd(serde_json::json!({ "cmd": "settings", "tab": "close" }));
    h.cmd(serde_json::json!({ "cmd": "library", "action": "open" }));
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    std::thread::sleep(Duration::from_millis(300));
    desk.capture(&dir.join("library-light.png"));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-E2E-ANKI-001 (L7): a real Anki with AnkiConnect, on a throwaway profile
// named by NIKOMIRU_ANKI_URL (`cargo xtask test --anki-e2e`)
#[test]
#[ignore = "L7: needs a running Anki with AnkiConnect (NIKOMIRU_ANKI_URL) and the L6 tools"]
fn l7_a_card_in_a_real_anki() {
    let url = std::env::var("NIKOMIRU_ANKI_URL")
        .expect("NIKOMIRU_ANKI_URL: the AnkiConnect of a throwaway Anki profile");
    let client = nikomiru_anki::Client::new(&url, "").unwrap();
    // Create Lapis deck, twice: idempotent
    nikomiru_anki::lapis::create(&client).unwrap();
    assert_eq!(
        nikomiru_anki::lapis::create(&client).map(|m| m.setup),
        Ok(nikomiru_anki::lapis::Setup::AlreadyExists)
    );
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[anki]\nurl = \"{url}\"\n"
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] != "unknown");
    // add anyway: the profile may hold the card of an earlier run
    h.cmd(serde_json::json!({ "cmd": "card", "level": 0, "entry": 0, "item": "add-anyway" }));
    let id = h.wait("card-created")["note_id"].as_u64().unwrap();
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let note = client.notes_info(&[id]).unwrap().remove(0).unwrap();
    assert_eq!(note.model, "Lapis");
    let field = |name: &str| {
        note.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(field("Expression"), "食べる");
    assert_eq!(field("Sentence"), "<b>食べました</b>");
    assert!(
        field("MiscInfo").contains("nikomiru://source?v=1"),
        "{}",
        field("MiscInfo")
    );
    // the media are in collection.media
    let names: Vec<String> = [field("SentenceAudio"), field("Picture")]
        .iter()
        .filter_map(|v| {
            let start = v.find("nikomiru_")?;
            let end = v[start..].find([']', '"'])?;
            Some(v[start..start + end].to_string())
        })
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(client.media_exist(&names).unwrap(), [true, true]);
}

// SET-002, T-SEC-008: a config.toml that cannot be read is never written over
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn anki_settings_do_not_overwrite_a_config_file_that_did_not_load() {
    let mut desk = Desktop::start();
    let broken = "[player]\nmpv_options = { ao = \"null\" }\nseek_seconds = \"five\"\n# the user's comment\n";
    desk.write_config(broken);
    let (mut app, mut h) = desk.launch();
    h.cmd(serde_json::json!({ "cmd": "anki", "action": "save", "config": "deck = \"Mining\"" }));
    h.state_until("not saved", |s| {
        s["anki_page"]["status"]["ok"] == false
            && s["anki_page"]["status"]["text"]
                .as_str()
                .is_some_and(|t| t.contains("config.toml"))
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let text = std::fs::read_to_string(desk.path("config/nikomiru/config.toml")).unwrap();
    assert_eq!(text, broken, "the user's file is untouched");
}

// T-PRV-002
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_logs_never_hold_an_api_key() {
    const SENTINEL: &str = "SENTINEL-4f2a9c";
    let mut desk = Desktop::start();
    // a key in its place and one in the wrong field, which makes the file invalid
    desk.write_config(&format!(
        "[anki]\napi_key = \"{SENTINEL}-key\"\n[player]\nseek_seconds = \"{SENTINEL}-misplaced\"\n"
    ));
    let (mut app, mut h) = desk.launch_with(&["--debug"]);
    let error = h.state()["error"].as_str().unwrap_or_default().to_string();
    assert!(
        error.contains("config.toml"),
        "the invalid settings are reported: {error:?}"
    );
    assert!(!error.contains(SENTINEL), "{error}");
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let logs = desk.log();
    assert!(logs.contains("Nikomiru"), "the logs were written");
    assert!(
        !logs.contains(SENTINEL),
        "a secret reached the logs:\n{logs}"
    );
}

// T-SEC-004
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon, `cargo xtask fixtures` and the release build"]
fn the_release_build_starts_and_quits() {
    // `cargo xtask ci --full` builds it just before; by hand:
    // `cargo build --release -p nikomiru-app`
    let release =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/nikomiru");
    assert!(
        release.exists(),
        "{} is missing: cargo build --release -p nikomiru-app",
        release.display()
    );
    let mut desk = Desktop::start();
    let mut app = desk
        .command(&release, &[&fixture("av_basic.mkv")])
        .spawn()
        .unwrap();
    support::until(
        Duration::from_secs(20),
        "the release build loads a file",
        || {
            if let Some(status) = app.try_wait().unwrap() {
                panic!("the release build exited: {status}\n{}", desk.log());
            }
            desk.log().contains("file loaded").then_some(())
        },
    );
    let kill = std::process::Command::new("kill")
        .args(["-TERM", &app.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let status = exit_status(&mut app, Duration::from_secs(15));
    assert!(status.success(), "{status}\n{}", desk.log());
}

fn subs(state: &serde_json::Value) -> &serde_json::Value {
    &state["subtitles"]
}

fn visible_text(state: &serde_json::Value) -> Vec<String> {
    subs(state)["visible"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str().map(str::to_string))
        .collect()
}

// T-SUB-009
// T-SUB-012
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn embedded_subtitles_are_drawn_clickable_and_navigable() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    let shown = h.wait("cue-shown");
    assert_eq!(shown["index"], 0);
    let s = h.state();
    assert_eq!(visible_text(&s), ["食べました"]);
    // the Japanese SRT track, extracted and drawn by Nikomiru (SUB-007, SUB-008)
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("track:"),
        "{s}"
    );
    assert_eq!(subs(&s)["cues"], 3);

    // a click on the second grapheme selects it and pauses (byte offset 3: べ)
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 1 }));
    let s = h.state_until("the pause after the click", |s| s["paused"] == true);
    assert_eq!(subs(&s)["selected"], serde_json::json!([0, 3]));
    // no dictionary: the popup says so; Esc closes it and playback resumes
    let s = h.state_until("the empty popup", |s| s["popups"][0]["empty"] == true);
    assert_eq!(s["popups"][0]["query"], "べました", "{s}");
    h.key("Escape");
    h.state_until("the popup closed and playback resumed", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && s["paused"] == false
    });

    // Ctrl+→ goes 180 ms before the next cue; paused, so that the position
    // stays there long enough to be read
    h.key("Space");
    h.state_until("paused", |s| s["paused"] == true);
    h.cmd(serde_json::json!({ "cmd": "key", "code": "ArrowRight", "mods": ["Ctrl"] }));
    let s = h.state_until("the jump to the next cue", |s| {
        (time_pos(s) - 4.82).abs() < 0.05
    });
    assert!(visible_text(&s).is_empty(), "{s}");
    // F enters fullscreen, Esc leaves it (M1 checklist, item 10)
    h.key("KeyF");
    h.state_until("fullscreen", |s| s["fullscreen"] == true);
    h.key("Escape");
    h.state_until("windowed again", |s| s["fullscreen"] == false);
    // V turns the subtitles off and on again, keeping the track
    h.key("KeyV");
    h.state_until("subtitles off", |s| subs(s)["disabled"] == true);
    h.key("KeyV");
    let s = h.state_until("subtitles on", |s| subs(s)["disabled"] == false);
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("track:"),
        "{s}"
    );
    // X moves the subtitles 100 ms later
    h.key("KeyX");
    let s = h.state_until("the offset", |s| subs(s)["delay_ms"] == 100);
    assert!(s["toast"].as_str().unwrap_or("").contains("+100"), "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-006
// T-SUB-008
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_subtitle_file_next_to_the_video_wins_over_embedded_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("Ep01.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(
        dir.path().join("Ep01.ja.srt"),
        "1\n00:00:01,000 --> 00:00:03,000\n<i>外部</i>の字幕ファイル\n",
    )
    .unwrap();
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&video.to_string_lossy());
    h.wait("file-loaded");
    h.wait("cue-shown");
    let s = h.state();
    assert_eq!(visible_text(&s), ["外部の字幕ファイル"]);
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .ends_with("Ep01.ja.srt"),
        "{s}"
    );
    assert_eq!(subs(&s)["encoding"], "UTF-8");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-006: names that differ from the video's
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn subtitles_of_the_same_episode_open_with_the_video() {
    let srt = |text: &str| format!("1\n00:00:01,000 --> 00:00:03,000\n{text}\n");
    let menu = |s: &serde_json::Value| -> Vec<String> {
        subs(s)["menu"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["kind"] == "file")
            .map(|m| m["name"].as_str().unwrap_or("").to_string())
            .collect()
    };
    // a release: the episode in a different name, another episode beside it
    let release = tempfile::tempdir().unwrap();
    let video = release.path().join("[Group] Show - 02 [1080p].mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(release.path().join("[Group] Show - 03 [1080p].mkv"), b"").unwrap();
    std::fs::write(release.path().join("Show 02.ja.srt"), srt("二話の字幕")).unwrap();
    std::fs::write(release.path().join("Show 03.ja.srt"), srt("三話の字幕")).unwrap();
    std::fs::create_dir(release.path().join("Subs")).unwrap();
    std::fs::write(
        release.path().join("Subs/Show_E02.en.srt"),
        srt("Episode two"),
    )
    .unwrap();
    // a film: its subtitles in a folder of its own
    let film = tempfile::tempdir().unwrap();
    let movie = film.path().join("Movie (2016).mkv");
    std::fs::copy(fixture("av_basic.mkv"), &movie).unwrap();
    std::fs::create_dir_all(film.path().join("Subs/Movie (2016)")).unwrap();
    std::fs::write(
        film.path().join("Subs/Movie (2016)/Japanese.srt"),
        srt("映画の字幕"),
    )
    .unwrap();
    std::fs::write(
        film.path().join("Subs/Movie (2016)/English.srt"),
        srt("The film"),
    )
    .unwrap();

    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&video.to_string_lossy());
    h.wait("file-loaded");
    h.wait("cue-shown");
    let s = h.state();
    assert_eq!(visible_text(&s), ["二話の字幕"], "{s}");
    assert_eq!(menu(&s), ["Show 02.ja.srt", "Show_E02.en.srt"], "{s}");

    h.open(&movie.to_string_lossy());
    h.wait("file-loaded");
    h.wait("cue-shown");
    let s = h.state();
    assert_eq!(visible_text(&s), ["映画の字幕"], "{s}");
    assert_eq!(menu(&s).len(), 2, "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-LIB-002, T-LIB-005: a folder imported from the Library page
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_folder_is_imported_and_played_from_the_library() {
    let mut desk = Desktop::start();
    let root = desk.path("videos");
    for name in [
        "Show/Season 1/Show - 01.mkv",
        "Show/Season 1/Show - 02.mkv",
        "Film.mkv",
    ] {
        let link = root.join(name);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(fixture("av_basic.mkv"), &link).unwrap();
    }
    let (mut app, mut h) = desk.launch();
    // no file given: the Library page, empty
    let s = h.state_until("the page", |s| s["library_page"]["series"].is_array());
    assert_eq!(s["library_page"]["open"], true);
    assert_eq!(s["library_page"]["series"], serde_json::json!([]));
    h.cmd(serde_json::json!({ "cmd": "library", "action": "import", "path": root }));
    let s = h.state_until("the series", |s| {
        s["library_page"]["series"]
            .as_array()
            .is_some_and(|a| a.len() == 2)
    });
    let series = s["library_page"]["series"].as_array().unwrap().clone();
    let titles: Vec<&str> = series
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Film", "Show"], "{s}");
    let show = series[1]["id"].as_u64().unwrap();
    assert_eq!(series[1]["episodes"], 2);
    // the covers, made with ffmpeg and kept
    h.state_until("the covers", |s| {
        s["library_page"]["covers"]
            .as_array()
            .is_some_and(|a| a.len() == 2)
    });
    let covers = std::fs::read_dir(desk.path("cache/nikomiru/covers"))
        .unwrap()
        .count();
    assert_eq!(covers, 2);
    h.cmd(serde_json::json!({ "cmd": "library", "action": "series", "id": show }));
    let s = h.state_until("the episodes", |s| {
        s["library_page"]["open_series"]["episodes"].is_array()
    });
    let episodes = s["library_page"]["open_series"]["episodes"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        (
            episodes[0]["season"].as_u64(),
            episodes[0]["episode"].as_u64()
        ),
        (Some(1), Some(1))
    );
    let first = episodes[0]["path"].as_str().unwrap().to_string();
    // played from the page: it closes, the video shows
    h.cmd(serde_json::json!({ "cmd": "library", "action": "play", "path": first }));
    h.wait("file-loaded");
    h.state_until("the page closed", |s| s["library_page"]["open"] == false);
    h.wait("first-frame");
    std::thread::sleep(Duration::from_millis(1_500));
    // Esc over the video: back to the library, where it is to continue
    h.key("Escape");
    let s = h.state_until("continue watching", |s| {
        s["library_page"]["continue"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    });
    assert_eq!(s["library_page"]["continue"][0], "video/Show - 01", "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-LIB-009, T-LIB-007, T-LIB-010: the next episode after a countdown Esc
// cancels, with the series' choices
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_next_episode_starts_after_a_countdown() {
    let mut desk = Desktop::start();
    let root = desk.path("videos");
    std::fs::create_dir_all(&root).unwrap();
    for name in ["Show - 01.mkv", "Show - 02.mkv"] {
        std::os::unix::fs::symlink(fixture("av_basic.mkv"), root.join(name)).unwrap();
    }
    let (mut app, mut h) = desk.launch();
    h.cmd(serde_json::json!({ "cmd": "library", "action": "import", "path": root }));
    h.state_until("the series", |s| {
        s["library_page"]["series"]
            .as_array()
            .is_some_and(|a| a.len() == 1)
    });
    h.cmd(serde_json::json!({
        "cmd": "library", "action": "play", "path": root.join("Show - 01.mkv")
    }));
    h.wait("file-loaded");
    h.wait("first-frame");
    // LIB-010: an offset set on episode 1 is the series'
    h.key("KeyX");
    let delay =
        h.state_until("the offset", |s| subs(s)["delay_ms"] != 0)["subtitles"]["delay_ms"].clone();
    // to the end: a countdown, Esc stops it
    for _ in 0..4 {
        h.key("ArrowRight");
    }
    h.wait("next-up");
    let s = h.state();
    assert_eq!(s["next_up"]["title"], "Show - 02", "{s}");
    h.key("Escape");
    h.state_until("cancelled", |s| s["next_up"].is_null());
    std::thread::sleep(Duration::from_secs(6));
    assert_eq!(h.state()["title"], "Show - 01.mkv");
    // back a little and played (the end paused it), to the end again: this
    // time it goes on
    h.key("ArrowLeft");
    h.key("Space");
    h.wait("next-up");
    let loaded = h.wait("file-loaded");
    assert!(loaded.is_object());
    // from its beginning (LIB-007), playing, with the series' offset (LIB-010)
    let s = h.state_until("episode 2 playing", |s| {
        s["title"] == "Show - 02.mkv" && s["paused"] == false && time_pos(s) < 2.0
    });
    assert_eq!(subs(&s)["delay_ms"], delay, "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-AUD-002, T-AUD-004, T-AUD-006: the word of the popup, played by itself
// and from its menu, from a source of the user's
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_word_is_played_from_the_popup() {
    let fake = nikomiru_online::fake::FakeOnline::start().unwrap();
    let mut desk = Desktop::start();
    // half a second of a tone, as a word's audio
    let wav = desk.path("word.wav");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=d=0.5",
        ])
        .args(["-ar", "8000", "-ac", "1"])
        .arg(&wav)
        .status()
        .unwrap();
    assert!(status.success());
    fake.data()
        .words
        .insert("食べる".into(), std::fs::read(&wav).unwrap());
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[[word_audio.sources]]\nname = \"Fake\"\n\
         url = \"{}/words/list?term={{term}}&reading={{reading}}\"\n",
        fake.proxy_url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    // by itself, when the popup opens (AUD-006)
    let played = h.wait("word-audio");
    assert_eq!(played["found"], true, "{played}");
    let asked: Vec<String> = fake.requests().into_iter().map(|r| r.target).collect();
    assert!(
        asked.iter().any(|t| t.contains(
            "/words/list?term=%E9%A3%9F%E3%81%B9%E3%82%8B&reading=%E3%81%9F%E3%81%B9%E3%82%8B"
        )),
        "{asked:?}"
    );
    assert!(
        asked.iter().any(|t| t.contains("/words/file/")),
        "{asked:?}"
    );
    // the menu: every source, then one of them played, from the cache
    h.cmd(serde_json::json!({ "cmd": "word_audio", "action": "menu", "level": 0, "entry": 0 }));
    let s = h.state_until("the menu", |s| s["popups"][0]["audio"]["items"].is_array());
    let items = s["popups"][0]["audio"]["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1, "{s}");
    assert_eq!(items[0]["label"], "Fake: fake");
    let files_before = fake
        .requests()
        .iter()
        .filter(|r| r.target.contains("/words/file/"))
        .count();
    h.cmd(serde_json::json!({ "cmd": "word_audio", "action": "url", "url": items[0]["url"] }));
    assert_eq!(h.wait("word-audio")["found"], true);
    let files_after = fake
        .requests()
        .iter()
        .filter(|r| r.target.contains("/words/file/"))
        .count();
    assert_eq!(files_before, files_after, "played from the cache (AUD-008)");
    // the video stays paused for the lookup
    assert_eq!(h.state()["paused"], true);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-AUD-003: the word from a database of the Local Audio Server
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_word_is_played_from_a_local_database() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[[word_audio.sources]]\nname = \"Local\"\n\
         url = \"{}\"\npriority = [\"jpod\", \"nhk16\"]\n",
        fixture("audio/android.db")
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    assert_eq!(h.wait("word-audio")["found"], true);
    h.cmd(serde_json::json!({ "cmd": "word_audio", "action": "menu", "level": 0, "entry": 0 }));
    let s = h.state_until("the menu", |s| s["popups"][0]["audio"]["items"].is_array());
    let labels: Vec<String> = s["popups"][0]["audio"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect();
    // the sub-sources of the settings first, then the others
    assert_eq!(
        labels,
        [
            "Local: jpod",
            "Local: nhk16",
            "Local: forvo",
            "Local: forvo"
        ],
        "{s}"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-STA-001, T-STA-002, T-STA-004, T-STA-005: what is watched is in the
// statistics, counted once
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn watching_fills_the_statistics() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("first-frame");
    // the first cue, 2–5 s, played whole, then a word looked up
    h.state_until("past the first cue", |s| time_pos(s) >= 5.5);
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.key("Escape");
    // Esc over the video: the library, its statistics with this session
    h.key("Escape");
    let s = h.state_until("the session", |s| {
        s["library_page"]["stats"]["sessions"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    });
    let session = &s["library_page"]["stats"]["sessions"][0];
    assert_eq!(session["name"], "av_basic", "{s}");
    let ms = session["ms"].as_i64().unwrap();
    assert!((4_000..=9_000).contains(&ms), "{session}");
    // 食べました, 5 characters; with 日本語を勉強しています, 11 more, if the
    // click came 1.5 s into the second cue
    let chars = session["chars"].as_i64().unwrap();
    assert!(chars == 5 || chars == 16, "{session}");
    assert_eq!(session["lookups"], 1, "{session}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PERF-07
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn perf_cues_appear_within_50_ms_of_their_start() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    for start in [2.0, 5.0, 8.0] {
        let shown = h.wait("cue-shown");
        let at = shown["time_pos"].as_f64().unwrap();
        assert!(
            (0.0..=0.050).contains(&(at - start)),
            "cue at {start} shown at {at}"
        );
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-POP-001, T-POP-002, T-POP-004 with real dictionaries
// T-PERF-06
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_click_on_a_subtitle_word_opens_the_dictionary_popup() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    // 食べました: the lookup finds 食べる, the popup is drawn, the video pauses
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    let rendered = match h.try_cmd(
        serde_json::json!({ "cmd": "wait", "event": "popup-rendered", "timeout_ms": 20000 }),
    ) {
        Ok(v) => v,
        Err(e) => panic!("{e}\nstate: {}\nlog:\n{}", h.state(), desk.log()),
    };
    assert_eq!(rendered["query"], "食べました");
    assert_eq!(rendered["headwords"][0], "食べる", "{rendered}");
    let s = h.state_until("paused for the lookup", |s| s["paused"] == true);
    assert_eq!(s["paused_for_lookup"], true);
    // the matched text is highlighted: the whole 食べました (15 bytes)
    assert_eq!(subs(&s)["highlight"], serde_json::json!([0, 0, 15]), "{s}");
    // Esc closes it and playback resumes (POP-004)
    h.key("Escape");
    h.state_until("resumed", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && s["paused"] == false
    });

    // T-PERF-06: from the click to the drawn popup, p95 ≤ 100 ms
    let mut times = Vec::new();
    for i in 0..30 {
        let started = std::time::Instant::now();
        h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": i % 3 }));
        h.wait("popup-rendered");
        times.push(started.elapsed());
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    eprintln!(
        "T-PERF-06 click → popup: median {:?}, p95 {p95:?}",
        times[times.len() / 2]
    );
    assert!(p95 <= Duration::from_millis(100), "p95 {p95:?}");
    // nothing went wrong in the interface while drawing them
    let log = desk.log();
    let bad: Vec<&str> = log
        .lines()
        .filter(|l| (l.contains("WARN") || l.contains("ERROR")) && l.contains("nikomiru_ui"))
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SEC-001, T-SEC-006
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_hostile_dictionary_entry_shows_only_what_the_allowlist_lets_through() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    // a spy proxy: anything that fetched an address of the entry through the
    // usual proxy variables would connect here
    let spy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    spy.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", spy.local_addr().unwrap());
    for var in [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
    ] {
        desk.set_env(var, &proxy);
    }
    let (mut app, mut h) = desk.launch();
    // 危険: a <script>, an onclick, a remote image and a javascript: link
    h.cmd(serde_json::json!({ "cmd": "lookup", "text": "危険" }));
    let rendered = h.wait("popup-rendered");
    assert_eq!(
        rendered["headwords"],
        serde_json::json!(["危険"]),
        "{rendered}"
    );
    let content = h.cmd(serde_json::json!({ "cmd": "popup_content", "level": 0 }));
    let section = &content["entries"][0]["sections"][0];
    assert_eq!(section["dictionary"], "Nikomiru mini-ja", "{content}");
    assert_eq!(
        section["glossaries"][0]["blocks"],
        serde_json::json!([{ "k": "flow", "runs": [
            { "k": "text", "t": "danger", "s": { "color": "#ff0000" } },
            { "k": "link", "t": "click", "target": { "kind": "lookup", "to": "click" }, "s": {} }
        ] }]),
        "only the span's text and colour and the link's text survive"
    );
    let text = content.to_string();
    for bad in [
        "alert",
        "steal",
        "evil.example",
        "javascript",
        "script",
        "url(",
    ] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
    // nothing reached for the network
    let accepted = spy.accept();
    assert!(
        accepted
            .as_ref()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::WouldBlock),
        "the spy proxy was reached: {accepted:?}"
    );
    let sockets = support::internet_sockets(app.id());
    assert!(sockets.is_empty(), "{sockets:#?}");
    assert!(
        !support::internet_sockets(std::process::id()).is_empty(),
        "the check sees the spy's own socket"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-DIC-001, T-DIC-032 from the page
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_dropped_dictionary_is_imported_and_used() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    // a zip dropped on the window goes to the dictionaries page
    h.drop_files(&[&fixture("dicts/mini-ja.zip")]);
    h.wait("dictionary-imported");
    let s = h.state();
    let rows = &s["dictionaries"]["rows"];
    assert_eq!(rows[0]["name"], "Nikomiru mini-ja", "{s}");
    assert_eq!(rows[0]["terms"], 76);
    assert_eq!(s["dictionaries"]["import"]["state"], "idle");
    // the same zip again: refused, nothing changes
    h.drop_files(&[&fixture("dicts/mini-ja.zip")]);
    let s = h.state_until("the refusal", |s| {
        s["toast"]
            .as_str()
            .unwrap_or("")
            .contains("Nikomiru mini-ja")
    });
    assert_eq!(s["dictionaries"]["rows"].as_array().map(Vec::len), Some(1));
    // Esc closes the page (it covers the popups); the popup uses the dictionary
    h.key("Escape");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    let rendered = h.wait("popup-rendered");
    assert_eq!(rendered["headwords"][0], "食べる", "{rendered}");
    h.key("Escape");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());

    // T-REL-001: the exit closed the library (the lookup worker holds it
    // too): the next start needs no repair and still has the dictionary
    let (mut app, mut h) = desk.launch();
    let s = h.state();
    assert!(
        !desk.data_file("library.redb.bak-repair").exists(),
        "the library was left open at exit"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert_eq!(
        s["dictionaries"]["rows"].as_array().map(Vec::len),
        Some(0),
        "the page is read when it opens"
    );
}

/// The style of the first text run of `content` that reads `text`.
fn style_of<'a>(content: &'a serde_json::Value, text: &str) -> Option<&'a serde_json::Value> {
    match content {
        serde_json::Value::Object(o) if o.get("t").and_then(|t| t.as_str()) == Some(text) => {
            o.get("s")
        }
        serde_json::Value::Object(o) => o.values().find_map(|v| style_of(v, text)),
        serde_json::Value::Array(a) => a.iter().find_map(|v| style_of(v, text)),
        _ => None,
    }
}

// T-DIC-035 with a real lookup
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_users_css_saved_on_the_page_styles_the_popup() {
    // 表 in mini-ja has a note, grey in the dictionary's styles.css
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("Ep01.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(
        dir.path().join("Ep01.ja.srt"),
        "1\n00:00:00,500 --> 00:00:09,500\n表\n",
    )
    .unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    // paused on the cue, so that it stays
    let open = |desk: &mut Desktop| {
        let (app, mut h) = desk.launch();
        h.open(&video.to_string_lossy());
        h.wait("file-loaded");
        h.state_until("the cue", |s| visible_text(s) == ["表"]);
        h.key("Space");
        h.state_until("paused", |s| s["paused"] == true);
        (app, h)
    };
    let (mut app, mut h) = open(&mut desk);
    let note = |h: &mut support::Harness| {
        h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
        h.wait("popup-rendered");
        let content = h.cmd(serde_json::json!({ "cmd": "popup_content", "level": 0 }));
        let style = style_of(&content, "a table").cloned();
        h.key("Escape");
        h.state_until("the popup closed", |s| {
            s["popups"].as_array().is_some_and(Vec::is_empty)
        });
        style.unwrap_or_else(|| panic!("no note in {content}"))
    };
    let style = note(&mut h);
    assert_eq!(style["color"], "#808080", "{style}");
    assert_eq!(style["italic"], true, "{style}");

    // the page, opened by a drop of the installed zip (refused as the same
    // revision); then its editor's Save
    h.drop_files(&[&fixture("dicts/mini-ja.zip")]);
    let s = h.state_until("the page's list", |s| {
        s["dictionaries"]["rows"]
            .as_array()
            .is_some_and(|r| !r.is_empty())
    });
    let id = s["dictionaries"]["rows"][0]["id"].clone();
    let css = r#"span[data-sc-class="note"] { color: #ff0000; position: fixed }"#;
    h.cmd(serde_json::json!({ "cmd": "dict_css", "id": id, "css": css }));
    let s = h.state();
    assert_eq!(s["dictionaries"]["rows"][0]["userCss"], css, "{s}");
    assert!(
        s["toast"]
            .as_str()
            .unwrap_or("")
            .contains("Nikomiru mini-ja"),
        "{s}"
    );
    // Esc closes the page, which covers the popups
    h.key("Escape");

    // the user's rule comes after the dictionary's and wins; the rest of the
    // style stays, and what the subset refuses does nothing
    let style = note(&mut h);
    assert_eq!(style["color"], "#ff0000", "{style}");
    assert_eq!(style["italic"], true, "{style}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());

    // kept for the next start
    let (mut app, mut h) = open(&mut desk);
    assert_eq!(note(&mut h)["color"], "#ff0000");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-POP-014, T-POP-015
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_popup_keyboard_replay_and_go_to_sentence() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    let cue_start = time_pos(&h.state());
    // 食べました: 食べる, 食べ物?, 食… several entries
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    let s = h.state();
    let entries = s["popups"][0]["entries"].as_u64().unwrap_or(0);
    assert!(entries >= 2, "{s}");
    h.key("Tab");
    h.state_until("the second entry", |s| s["popups"][0]["entry"] == 1);
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Tab", "mods": ["Shift"] }));
    h.state_until("back to the first", |s| s["popups"][0]["entry"] == 0);

    // R with the popup open: the cue again, pausing at its end (SUB-013)
    h.key("KeyR");
    let s = h.state_until("replaying", |s| s["paused"] == false);
    assert_eq!(
        s["popups"].as_array().map(Vec::len),
        Some(1),
        "the popup stays"
    );
    // the cue is 2.0–2.5 s: mpv stops on its last frame
    let s = h.state_until("paused at the end of the cue", |s| {
        s["paused"] == true && time_pos(s) >= cue_start + 0.3
    });
    assert!(time_pos(&s) <= 2.5, "stopped at the end of the cue: {s}");
    // closing resumes and the stop is gone: playback goes past the cue
    h.key("Escape");
    h.state_until("past the end of the cue", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && time_pos(s) > 5.2
    });
    // "go to the sentence": the popups close and playback is back at the cue
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 1, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.cmd(serde_json::json!({ "cmd": "popup_button", "button": "goto" }));
    h.state_until("back at the second cue, playing", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty)
            && s["paused"] == false
            && (4.9..5.6).contains(&time_pos(s))
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-LIB-001, T-DIC-001 on Wayland, where winit takes no drops: the app's own
// data device, with a drag from another client as a file manager makes it
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn files_dragged_from_another_client_open_and_import() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    let mut source = drag::DragSource::connect(&desk.wayland_socket());
    // a video: it opens, and is copied (the source offers a move too)
    let video = fixture("-weird: name & spaces ü 日本.mkv");
    let outcome = source.drag(&[&video], (640.0, 400.0));
    assert_eq!(
        outcome.accepted.as_deref(),
        Some("text/uri-list"),
        "{outcome:?}"
    );
    assert!(
        outcome.sent && outcome.finished && !outcome.cancelled,
        "{outcome:?}"
    );
    h.wait("file-loaded");
    let s = h.state();
    assert!(s["title"].as_str().unwrap_or("").contains("weird"), "{s}");
    assert!(std::path::Path::new(&video).exists(), "copied, not moved");
    // a dictionary: imported from the page
    let outcome = source.drag(&[&fixture("dicts/mini-ja.zip")], (640.0, 400.0));
    assert!(outcome.finished, "{outcome:?}");
    h.wait("dictionary-imported");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-REL-001, T-DIC-010: quitting during an import cancels it; the library is
// closed, nothing is registered and nothing is left to repair or clean up
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn quitting_during_an_import_leaves_nothing_behind() {
    let mut desk = Desktop::start();
    let zip = desk.large_dictionary(300_000);
    let (mut app, mut h) = desk.launch();
    h.drop_files(&[&zip]);
    h.state_until("the import running", |s| {
        s["dictionaries"]["import"]["state"] == "running"
            && s["dictionaries"]["import"]["done"].as_u64().unwrap_or(0) > 0
    });
    let quitting = std::time::Instant::now();
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let took = quitting.elapsed();
    eprintln!("quit during the import: {took:?}");
    assert!(took < Duration::from_secs(5), "the exit waited {took:?}");
    assert!(
        !desk.data_file("library.redb.bak-repair").exists(),
        "the library was left open"
    );
    // the next start: still nothing to repair, nothing registered, no
    // staging folder left
    let (mut app, mut h) = desk.launch();
    h.state();
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert!(!desk.data_file("library.redb.bak-repair").exists());
    assert!(
        desk.registered_dictionaries().is_empty(),
        "{:?}",
        desk.registered_dictionaries()
    );
    let left: Vec<_> = std::fs::read_dir(desk.data_file("dicts"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
}

/// A Japanese subtitle file the fake online service gives.
const ONLINE_SRT: &str =
    "1\n00:00:00,500 --> 00:00:04,000\n食べました\n\n2\n00:00:05,000 --> 00:00:08,000\n猫です\n";

fn online_fake() -> nikomiru_online::fake::FakeOnline {
    let fake = nikomiru_online::fake::FakeOnline::start().unwrap();
    {
        let mut d = fake.data();
        d.entries = vec![(1, "av long".into(), true)];
        d.files.insert(
            1,
            vec![("av long.ja.srt".into(), ONLINE_SRT.as_bytes().to_vec())],
        );
    }
    fake
}

/// The files kept under `subtitles/<hash>/` of the data folder.
fn kept_subtitles(desk: &Desktop) -> Vec<std::path::PathBuf> {
    let root = desk.data_file("subtitles");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(dir.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            out.push(f.path());
        }
    }
    out
}

// T-MET-007, T-MET-008
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn subtitles_come_online_for_a_video_without_them() {
    let fake = online_fake();
    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[online]\nauto_subtitles = true\njimaku_api_key = \"jimaku-key\"\njimaku_url = \"{}\"\n",
        fake.jimaku_url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("file-loaded");
    let s = h.state_until("the online subtitles", |s| s["subtitles"]["cues"] == 2);
    let source = s["subtitles"]["source"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(source.contains("/subtitles/"), "{s}");
    let kept = kept_subtitles(&desk);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].file_name().unwrap(), "av_long.ja.srt");
    assert!(
        fake.requests()
            .iter()
            .all(|r| r.header("authorization") == Some("jimaku-key"))
    );
    // the next opening finds them without the network
    fake.clear_requests();
    h.open(&fixture("av_long.mkv"));
    h.wait("file-loaded");
    h.state_until("the kept subtitles", |s| s["subtitles"]["cues"] == 2);
    assert!(fake.requests().is_empty(), "{:?}", fake.requests());
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-MET-009, PRV-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn subtitles_are_searched_and_used_on_the_online_page() {
    let fake = online_fake();
    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[online]\njimaku_api_key = \"jimaku-key\"\njimaku_url = \"{}\"\n",
        fake.jimaku_url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("file-loaded");
    h.wait("first-frame");
    // automatic subtitles are off: nothing was asked
    assert!(fake.requests().is_empty(), "{:?}", fake.requests());
    h.cmd(serde_json::json!({ "cmd": "online", "action": "open" }));
    let s = h.state_until("the page", |s| s["online_page"]["open"] == true);
    assert_eq!(s["online_page"]["query"]["title"], "av long", "{s}");
    h.cmd(serde_json::json!({ "cmd": "online", "action": "search" }));
    let s = h.state_until("the results", |s| {
        s["online_page"]["results"]["files"]
            .as_array()
            .is_some_and(|f| !f.is_empty())
    });
    let first = &s["online_page"]["results"]["files"][0];
    assert_eq!(
        (first["provider"].as_str(), first["name"].as_str()),
        (Some("Jimaku"), Some("av long.ja.srt"))
    );
    h.cmd(serde_json::json!({ "cmd": "online", "action": "use", "index": 0 }));
    h.state_until("loaded, the page closed", |s| {
        s["subtitles"]["cues"] == 2 && s["online_page"]["open"] == false
    });
    assert_eq!(kept_subtitles(&desk).len(), 1);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-MET-011, T-SET-008: only the services chosen are asked, set in the settings
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn only_the_subtitle_services_chosen_are_searched() {
    let fake = online_fake();
    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[online]\nproviders = [\"jimaku\"]\n\
         jimaku_api_key = \"jimaku-key\"\njimaku_url = \"{}\"\n\
         opensubtitles_api_key = \"os-key\"\nopensubtitles_url = \"{}\"\n",
        fake.jimaku_url(),
        fake.opensubtitles_url()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_long.mkv"));
    h.wait("file-loaded");
    let asked_os = |fake: &nikomiru_online::fake::FakeOnline| {
        fake.requests().iter().any(|r| r.target.starts_with("/os/"))
    };
    let search = |h: &mut support::Harness| {
        h.cmd(serde_json::json!({ "cmd": "online", "action": "open" }));
        h.cmd(serde_json::json!({ "cmd": "online", "action": "search" }));
        h.state_until("the results", |s| {
            s["online_page"]["busy"] == false && !s["online_page"]["results"].is_null()
        });
        h.cmd(serde_json::json!({ "cmd": "online", "action": "close" }));
    };
    search(&mut h);
    assert!(!fake.requests().is_empty());
    assert!(!asked_os(&fake), "{:?}", fake.requests());
    // the services are a tab of the settings
    h.cmd(serde_json::json!({ "cmd": "settings", "tab": "services" }));
    let s = h.state_until("the tab", |s| s["services_page"]["open"] == true);
    assert_eq!(s["settings_tab"], "services");
    assert_eq!(
        s["services_page"]["providers"],
        serde_json::json!(["jimaku"])
    );
    h.cmd(serde_json::json!({ "cmd": "services", "providers": ["opensubtitles", "jimaku"] }));
    h.state_until("saved", |s| {
        s["services_page"]["status"]["ok"] == true
            && s["services_page"]["providers"] == serde_json::json!(["opensubtitles", "jimaku"])
    });
    let file = desk.path("config/nikomiru/config.toml");
    let saved = nikomiru_core::Config::from_toml(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved.online.providers, ["opensubtitles", "jimaku"]);
    h.cmd(serde_json::json!({ "cmd": "settings", "tab": "close" }));
    search(&mut h);
    assert!(asked_os(&fake), "{:?}", fake.requests());
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PLY-016, T-PLY-018
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn an_mpv_conf_is_previewed_applied_saved_and_exported() {
    let mut desk = Desktop::start();
    desk.write_config("[player]\nmpv_options = { ao = \"null\" }\n");
    let conf = desk.path("mpv.conf");
    std::fs::write(
        &conf,
        "# a user's mpv.conf\nbrightness=10\ndeband\nscript=evil.lua\nvideo-zoom=0.25\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("first-frame");
    let mpv = |h: &mut support::Harness, p: &str| {
        h.cmd(serde_json::json!({ "cmd": "mpv_get", "property": p }))
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    h.cmd(serde_json::json!({ "cmd": "player", "action": "open" }));
    h.state_until("the page", |s| s["player_page"]["open"] == true);
    h.cmd(serde_json::json!({ "cmd": "player", "action": "import", "path": conf }));
    let s = h.state_until("the preview", |s| !s["player_page"]["preview"].is_null());
    let lines = s["player_page"]["preview"]["lines"]
        .as_array()
        .unwrap()
        .clone();
    let status = |text: &str| {
        lines
            .iter()
            .find(|l| l["text"] == text)
            .map(|l| l["status"].as_str().unwrap_or_default().to_string())
            .unwrap_or_default()
    };
    assert_eq!(status("brightness=10"), "Control", "{lines:?}");
    assert_eq!(status("video-zoom=0.25"), "Raw", "{lines:?}");
    assert!(status("script").starts_with("Ignored"), "{lines:?}");
    // nothing changes before it is applied
    let number = |v: String| v.parse::<f64>().unwrap_or(f64::NAN);
    assert_eq!(number(mpv(&mut h, "brightness")), 0.0);
    h.cmd(serde_json::json!({ "cmd": "player", "action": "preview-apply" }));
    h.state_until("applied", |s| s["player_page"]["set"]["brightness"] == "10");
    support::until(Duration::from_secs(10), "mpv takes the options", || {
        (number(mpv(&mut h, "brightness")) == 10.0 && number(mpv(&mut h, "video-zoom")) == 0.25)
            .then_some(())
    });
    assert_eq!(mpv(&mut h, "deband"), "yes");
    // saved in config.toml: the whole set replaced (PLY-016), `ao` gone
    let file = desk.path("config/nikomiru/config.toml");
    support::until(Duration::from_secs(10), "the settings saved", || {
        let text = std::fs::read_to_string(&file).ok()?;
        let c = nikomiru_core::Config::from_toml(&text).ok()?;
        (c.player.mpv_options.get("brightness").map(String::as_str) == Some("10")).then_some(c)
    });
    // a preset, and an export mpv reads
    h.cmd(serde_json::json!({ "cmd": "player", "action": "save-preset", "name": "anime" }));
    h.state_until("the preset", |s| {
        s["player_page"]["presets"] == serde_json::json!(["anime"])
    });
    let preset = std::fs::read_to_string(desk.path("config/nikomiru/mpv/anime.conf")).unwrap();
    assert!(preset.starts_with("# Nikomiru "), "{preset}");
    let out = desk.path("exported.conf");
    h.cmd(serde_json::json!({ "cmd": "player", "action": "export", "path": out }));
    let text = support::until(Duration::from_secs(10), "the export", || {
        std::fs::read_to_string(&out).ok()
    });
    for line in ["brightness=10", "deband=yes", "video-zoom=0.25"] {
        assert!(text.lines().any(|l| l == line), "{line} in {text}");
    }
    assert!(!text.contains("script") && !text.contains("ao="), "{text}");
    // the brightness unset: mpv's own value again
    h.cmd(serde_json::json!({ "cmd": "player", "action": "load-preset", "name": "anime" }));
    h.state_until("the preset's preview", |s| {
        !s["player_page"]["preview"].is_null()
    });
    h.cmd(serde_json::json!({ "cmd": "player", "action": "preview-cancel" }));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PLY-016: the user's own mpv.conf, while Nikomiru sets no option
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_users_mpv_conf_is_offered_on_the_player_page() {
    let mut desk = Desktop::start();
    // no video plays: no `ao=null` needed, and Nikomiru sets no option
    desk.write_config("");
    let folder = desk.path("config/mpv");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("mpv.conf"), "brightness=5\nscript=x.lua\n").unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(serde_json::json!({ "cmd": "player", "action": "open" }));
    let s = h.state_until("the offer", |s| !s["player_page"]["preview"].is_null());
    let page = &s["player_page"];
    assert!(
        page["preview"]["source"]
            .as_str()
            .unwrap_or_default()
            .ends_with("config/mpv/mpv.conf"),
        "{page}"
    );
    assert_eq!(page["status"]["ok"], true, "{page}");
    assert_eq!(
        page["import_folder"].as_str(),
        Some(folder.to_string_lossy().as_ref())
    );
    h.cmd(serde_json::json!({ "cmd": "player", "action": "preview-apply" }));
    h.state_until("applied", |s| s["player_page"]["set"]["brightness"] == "5");
    // now Nikomiru has its own set: the page opens on it, with no offer
    h.cmd(serde_json::json!({ "cmd": "player", "action": "close" }));
    h.cmd(serde_json::json!({ "cmd": "player", "action": "open" }));
    let s = h.state_until("the page", |s| s["player_page"]["open"] == true);
    assert!(s["player_page"]["preview"].is_null(), "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-018: subtitles written 1.2 s early are put back on the audio
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn subtitles_are_aligned_with_the_audio() {
    let mut desk = Desktop::start();
    desk.write_config("[player]\nmpv_options = { ao = \"null\" }\n");
    let early = desk.path("early.srt");
    std::fs::write(
        &early,
        "1\n00:00:00,800 --> 00:00:01,300\n食べました\n\n2\n00:00:03,800 --> 00:00:04,300\n日本語\n\n3\n00:00:06,800 --> 00:00:07,300\nラーメン\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.cmd(serde_json::json!({ "cmd": "drop", "paths": [early] }));
    h.state_until("the early subtitles", |s| {
        s["subtitles"]["source"]
            .as_str()
            .is_some_and(|k| k.contains("early.srt"))
            && s["subtitles"]["cues"] == 3
    });
    h.cmd(serde_json::json!({ "cmd": "align" }));
    let s = h.state_until("aligned", |s| s["subtitles"]["delay_ms"] != 0);
    let delay = s["subtitles"]["delay_ms"].as_i64().unwrap();
    assert!((delay - 1_200).abs() <= 100, "{s}");
    assert!(
        s["toast"].as_str().unwrap_or_default().contains("ms"),
        "{s}"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-017: a second track under the first, following its offset
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_secondary_subtitle_follows_the_main_one() {
    let mut desk = Desktop::start();
    desk.write_config("[player]\nmpv_options = { ao = \"null\" }\n");
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    let s = h.state_until("the main subtitles", |s| s["subtitles"]["cues"] == 3);
    let main = s["subtitles"]["source"].as_str().unwrap().to_string();
    let other = if main == "track:1" {
        "track:2"
    } else {
        "track:1"
    };
    h.cmd(serde_json::json!({ "cmd": "secondary", "key": other, "obscure": "blur" }));
    h.state_until("the secondary", |s| s["secondary"]["cues"] == 3);
    // its own obscuring, the main one's offset
    h.key("KeyX");
    let s = h.state_until("the offset followed", |s| {
        s["subtitles"]["delay_ms"] != 0 && s["secondary"]["delay_ms"] == s["subtitles"]["delay_ms"]
    });
    assert_eq!(s["subtitles"]["obscure"], "none", "{s}");
    assert_eq!(s["secondary"]["source"], other, "{s}");
    // shown with the main cue
    h.wait("cue-shown");
    let s = h.state_until("both shown", |s| {
        s["secondary"]["visible"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    });
    assert_eq!(s["secondary"]["obscure"], "blur", "{s}");
    // off again
    h.cmd(serde_json::json!({ "cmd": "secondary", "key": "off" }));
    h.state_until("no secondary", |s| s["secondary"]["source"].is_null());
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SET-002, T-SET-004, T-SET-005: the settings apply at once, the interface
// scale leaves the subtitles' size alone, the user's file keeps its comments
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_settings_apply_without_a_restart() {
    let mut desk = Desktop::start();
    desk.write_config(
        "# my settings\n[general]\nui_language = \"en\"\ntheme = \"dark\" # at night\n\
         [player]\nmpv_options = { ao = \"null\" }\n",
    );
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    let px = |s: &serde_json::Value| s["look"]["subtitle_px"].as_f64().unwrap_or(0.0);
    let s = h.state_until("the subtitles drawn", |s| px(s) > 0.0);
    let before = px(&s);
    assert_eq!(s["look"]["dark"], true, "{s}");
    assert_eq!(s["look"]["language"], "en", "{s}");
    assert_eq!(s["look"]["zoom"], 1.0, "{s}");
    // light, in Italian, at 150 %
    h.cmd(serde_json::json!({
        "cmd": "settings",
        "tab": "general",
        "save": "[general]\ntheme = \"light\"\nui_language = \"it\"\nui_scale = 150\n",
    }));
    let s = h.state_until("the new look", |s| s["look"]["zoom"] == 1.5);
    assert_eq!(s["look"]["dark"], false, "{s}");
    assert_eq!(s["look"]["language"], "it", "{s}");
    assert_eq!(s["settings_status"]["ok"], true, "{s}");
    // the subtitles keep their size on the screen
    assert!((px(&s) - before).abs() < 1.0, "{before} → {}", px(&s));
    // and grow at once with their own setting
    h.cmd(serde_json::json!({
        "cmd": "settings",
        "tab": "subtitles",
        "save": "[subtitles]\nfont_size = 54\n",
    }));
    h.state_until("bigger subtitles", |s| (px(s) - before * 1.5).abs() < 1.5);
    h.cmd(serde_json::json!({ "cmd": "settings", "tab": "close" }));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let text = std::fs::read_to_string(desk.path("config/nikomiru/config.toml")).unwrap();
    assert!(text.starts_with("# my settings\n"), "{text}");
    assert!(text.contains("theme = \"light\" # at night\n"), "{text}");
    assert!(text.contains("ui_scale = 150"), "{text}");
    assert!(text.contains("font_size = 54"), "{text}");
}
