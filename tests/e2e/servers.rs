// SPDX-License-Identifier: GPL-3.0-only

//! L6: a media server's video in the real app, against the fake servers
//! (TEST §6.13, M11).

use std::time::Duration;

use nikomiru_online::fake_servers::{Data, FakeItem, FakeServers, TOKEN};
use serde_json::json;

use crate::support::{Desktop, exit_status, fixture, until};

fn episode(n: u32, position_ms: u64) -> FakeItem {
    FakeItem {
        id: format!("e{n}"),
        kind: "Episode".into(),
        title: format!("Episode {n}"),
        parent: "season".into(),
        index: n,
        position_ms,
        last_played_s: Some(100),
    }
}

fn fake() -> FakeServers {
    FakeServers::start(Data {
        libraries: vec![("lib".into(), "Anime".into())],
        items: vec![episode(1, 2000), episode(2, 0)],
        file: std::fs::read(fixture("av_basic.mkv")).unwrap(),
        subtitle: b"1\n00:00:00,000 --> 00:01:00,000\nsubtitle of the server\n".to_vec(),
        ..Data::default()
    })
    .unwrap()
}

fn config(kind: &str, url: &str) -> String {
    format!(
        "[network]\ndevice_id = \"device\"\n\n[player]\nmpv_options = {{ ao = \"null\" }}\n\n\
         [[servers]]\nid = \"home\"\nkind = \"{kind}\"\nname = \"Home\"\nurl = \"{url}\"\n\
         token = \"{TOKEN}\"\nuser_id = \"user-1\"\n"
    )
}

fn plays_from(kind: &str, url: String, fake: &FakeServers, stopped: &str) {
    let mut desk = Desktop::start();
    desk.write_config(&config(kind, &url));
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    // T-SRV-004: nothing goes out for a server not in use
    assert!(fake.requests().is_empty(), "{:?}", fake.requests());
    h.open("srv:home:e1");
    h.wait("first-frame");
    let state = h.state_until("the video", |s| s["loaded"] == true);
    assert_eq!(state["title"], "Episode 1");
    // T-SRV-004: mpv sees the relay, never the server or the token
    let path = h.cmd(json!({ "cmd": "mpv_property", "name": "path" }));
    let path = path.as_str().unwrap();
    assert!(path.starts_with("http://127.0.0.1:"), "{path}");
    assert!(
        !path.contains(TOKEN) && !path.contains("/jf/") && !path.contains("/px/"),
        "{path}"
    );
    let line = std::fs::read(format!("/proc/{}/cmdline", app.id())).unwrap();
    assert!(!String::from_utf8_lossy(&line).contains(TOKEN));
    // T-SRV-011: the later position is the server's
    until(Duration::from_secs(5), "the resume seek", || {
        let at = h.cmd(json!({ "cmd": "mpv_property", "name": "time-pos" }));
        let at: f64 = at.as_str()?.parse().ok()?;
        (at >= 1.9).then_some(())
    });
    // T-SRV-010: its external subtitle is offered
    h.state_until("the server's subtitle", |s| {
        s["subtitles"]["menu"].to_string().contains("server-0")
    });
    // T-SRV-008: to a local file: the server hears the video stopped
    h.open(&fixture("av_basic.mkv"));
    h.wait("first-frame");
    until(Duration::from_secs(5), "the last report", || {
        fake.requests()
            .iter()
            .any(|r| r.target.contains(stopped))
            .then_some(())
    });
    assert!(!desk.log().contains(TOKEN), "the token in the log");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SRV-004, T-SRV-008, T-SRV-010, T-SRV-011
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_jellyfin_video_plays_through_the_relay() {
    let fake = fake();
    plays_from(
        "jellyfin",
        fake.jellyfin_url(),
        &fake,
        "/Sessions/Playing/Stopped",
    );
}

// T-SRV-004, T-SRV-008, T-SRV-010, T-SRV-011
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_plex_video_plays_through_the_relay() {
    let fake = fake();
    plays_from("plex", fake.plex_url(), &fake, "state=stopped");
}

// T-SRV-012: a link to a server that is gone, or a video it no longer has
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_server_that_is_gone_is_only_a_message() {
    let fake = fake();
    let mut desk = Desktop::start();
    desk.write_config(&config("jellyfin", &fake.jellyfin_url()));
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.open("srv:home:nothing");
    h.state_until("the message", |s| s["toast"].is_string());
    assert_eq!(h.state()["loaded"], false);
    let first = h.state()["toast"].clone();
    h.open("srv:other:e1");
    h.state_until("the second message", |s| s["toast"] != first);
    assert_eq!(h.state()["loaded"], false);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-BAK-009, T-PRV-001: with "Sync at start and close", this device's file
// reaches the folder; another's progress reaches the library, and nothing
// else is asked of the server
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_device_syncs_at_start_and_close() {
    use nikomiru_library::sync::{DeviceState, FORMAT, VideoState};
    let fake = fake();
    let mut other = DeviceState {
        format: FORMAT,
        ..DeviceState::default()
    };
    let video = fixture("av_basic.mkv");
    other.videos.insert(
        "video/av_basic".into(),
        VideoState {
            position_ms: 3000,
            last_played_at: Some(i64::MAX / 2),
            ..VideoState::default()
        },
    );
    fake.data()
        .dav_files
        .insert("other.sync".into(), other.encode().unwrap());
    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[network]\ndevice_id = \"mine\"\n\n[player]\nmpv_options = {{ ao = \"null\" }}\n\n\
         [sync]\nurl = \"{}\"\nuser = \"dav\"\npassword = \"pass\"\nautomatic = true\n",
        fake.webdav_url()
    ));
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    until(Duration::from_secs(10), "this device's file", || {
        fake.data()
            .dav_files
            .contains_key("mine.sync")
            .then_some(())
    });
    h.open(&video);
    h.wait("first-frame");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(40)).success());
    let bytes = fake.data().dav_files["mine.sync"].clone();
    let mine = DeviceState::decode(&bytes).unwrap();
    assert_eq!(mine.videos["video/av_basic"].position_ms, 3000, "{mine:?}");
}

// T-HOK-001, T-HOK-004, T-HOK-005: the lines of a game, a word of one looked
// up, and a card with no video, only text
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_line_of_a_game_becomes_a_card() {
    use nikomiru_anki::fake::FakeAnki;
    use nikomiru_online::fake_servers::FakeHooker;
    let anki = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&anki.url(), "").unwrap()).unwrap();
    let hooker = FakeHooker::start(vec!["食べました".into()]).unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    desk.write_config(&format!(
        "[hooker]\nurl = \"{}\"\n[anki]\nurl = \"{}\"\n",
        hooker.url(),
        anki.url()
    ));
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    // T-HOK-001: nothing connects until the page is open
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(hooker.connections(), 0);
    h.cmd(json!({ "cmd": "hooker", "action": "open" }));
    h.state_until("the line", |s| {
        s["hooker"]["connected"] == true && s["hooker"]["lines"][0] == "食べました"
    });
    h.cmd(json!({ "cmd": "hooker", "action": "click", "line": 0 }));
    h.wait("popup-rendered");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    let created = h.cmd(json!({ "cmd": "wait", "event": "card-created", "timeout_ms": 20000 }));
    let note_id = created["note_id"].as_u64().unwrap();
    {
        let collection = anki.collection();
        let note = collection.notes.iter().find(|n| n.id == note_id).unwrap();
        assert!(
            note.fields
                .iter()
                .any(|(name, value)| name == "Sentence" && value.contains("ました")),
            "{:?}",
            note.fields
        );
        assert!(collection.media.is_empty(), "no picture, no audio");
    }
    // T-HOK-001: closing it ends the reading, and the app does not come back
    h.key("Escape");
    h.key("Escape");
    h.state_until("the page closed", |s| s["hooker"]["connected"] == false);
    let connections = hooker.connections();
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(hooker.connections(), connections);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-BOK-001, T-BOK-004, T-BOK-005, T-BOK-006, T-BOK-007: a book from its file
// to a card of its sentence, and back where it was left
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_book_is_read_and_mined_and_keeps_its_place() {
    use nikomiru_anki::fake::FakeAnki;
    let anki = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&anki.url(), "").unwrap()).unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    desk.write_config(&format!("[anki]\nurl = \"{}\"\n", anki.url()));
    let path = desk.path("book.epub");
    nikomiru_books::sample::write(&path).unwrap();
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.cmd(json!({ "cmd": "books", "action": "open" }));
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the book", |s| s["books"]["open"] == "見本の本");
    h.cmd(json!({ "cmd": "books", "action": "next" }));
    h.state_until("the second chapter", |s| s["books"]["chapter"] == 1);
    // 明日も食べました: the click is on 食, three characters in
    h.cmd(json!({ "cmd": "books", "action": "click", "paragraph": 1, "offset": 9 }));
    h.wait("popup-rendered");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    let created = h.cmd(json!({ "cmd": "wait", "event": "card-created", "timeout_ms": 20000 }));
    let note_id = created["note_id"].as_u64().unwrap();
    {
        let collection = anki.collection();
        let note = collection.notes.iter().find(|n| n.id == note_id).unwrap();
        assert!(
            note.fields
                .iter()
                .any(|(name, value)| name == "Sentence" && value.contains("ました")),
            "{:?}",
            note.fields
        );
        assert!(collection.media.is_empty(), "no picture, no audio");
    }
    h.cmd(json!({ "cmd": "books", "action": "close" }));
    h.state_until("its place", |s| {
        s["books"]["open"].is_null() && s["books"]["list"][0]["chapter"] == 1
    });
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the same chapter", |s| s["books"]["chapter"] == 1);
    assert_eq!(h.state()["books"]["list"].as_array().unwrap().len(), 1);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    // T-BOK-008: each closing added a session, the first with what it advanced
    let library = nikomiru_library::Library::open(&desk.data_file("library.redb")).unwrap();
    let sessions = library.segments().unwrap();
    assert_eq!(sessions.len(), 2, "{sessions:?}");
    assert!(sessions.iter().all(|(_, s)| s.media.starts_with("book/")));
    assert!(sessions.iter().any(|(_, s)| s.chars > 0), "{sessions:?}");
}

// T-SRV-LIVE-002: the app plays an episode of a real Jellyfin, from
// `NIKOMIRU_JELLYFIN_URL`, `_USER` and `_PASSWORD`
#[test]
#[ignore = "L7: needs a real Jellyfin"]
fn l7_the_app_plays_from_a_real_jellyfin() {
    use nikomiru_online::http::Http;
    use nikomiru_online::servers::jellyfin::Jellyfin;
    use nikomiru_online::servers::{MediaServer, ParentKind, checked_server_url};
    let var = |name: &str| std::env::var(format!("NIKOMIRU_JELLYFIN_{name}")).ok();
    let (Some(url), Some(user), Some(password)) = (var("URL"), var("USER"), var("PASSWORD")) else {
        eprintln!("skipped: no NIKOMIRU_JELLYFIN_URL");
        return;
    };
    let base = checked_server_url(&url, false).unwrap();
    let mut server =
        Jellyfin::login(Http::new("").unwrap(), base, &user, &password, "device").unwrap();
    let library = server.libraries().unwrap().remove(0);
    let series = server
        .children(&library.id, 0, ParentKind::Library)
        .unwrap();
    let seasons = server
        .children(&series.items[0].id, 0, ParentKind::Item)
        .unwrap();
    let episodes = server
        .children(&seasons.items[0].id, 0, ParentKind::Item)
        .unwrap();
    let episode = &episodes.items[0];
    let (token, user_id) = (server.token().to_string(), server.user_id().to_string());
    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[network]\ndevice_id = \"device\"\n\n[player]\nmpv_options = {{ ao = \"null\" }}\n\n\
         [[servers]]\nid = \"home\"\nkind = \"jellyfin\"\nname = \"Home\"\nurl = \"{url}\"\n\
         token = \"{token}\"\nuser_id = \"{user_id}\"\n"
    ));
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.open(&format!("srv:home:{}", episode.id));
    h.wait("first-frame");
    let state = h.state_until("the video", |s| s["loaded"] == true);
    assert_eq!(state["title"], episode.title.as_str());
    let codec = h.cmd(json!({ "cmd": "mpv_property", "name": "video-codec" }));
    eprintln!("playing {codec}");
    h.state_until("the server's subtitle", |s| {
        s["subtitles"]["menu"].to_string().contains("server-0")
    });
    h.state_until("the duration", |s| {
        s["duration"].as_f64().unwrap_or(0.0) > 1.0
    });
    assert!(!desk.log().contains(&token), "the token in the log");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-BAK-LIVE-002: the app syncs with a real WebDAV folder, from
// `NIKOMIRU_WEBDAV_URL`, `_USER` and `_PASSWORD`
#[test]
#[ignore = "L7: needs a real WebDAV server"]
fn l7_the_app_syncs_with_a_real_webdav() {
    use nikomiru_library::sync::{DeviceState, FORMAT, VideoState};
    use nikomiru_online::webdav::WebDav;
    let var = |name: &str| std::env::var(format!("NIKOMIRU_WEBDAV_{name}")).ok();
    let (Some(url), Some(user), Some(password)) = (var("URL"), var("USER"), var("PASSWORD")) else {
        eprintln!("skipped: no NIKOMIRU_WEBDAV_URL");
        return;
    };
    let dav = WebDav::new(&url, &user, &password, false, "").unwrap();
    dav.prepare().unwrap();
    let mut other = DeviceState {
        format: FORMAT,
        ..DeviceState::default()
    };
    other.videos.insert(
        "video/av_basic".into(),
        VideoState {
            position_ms: 3000,
            last_played_at: Some(i64::MAX / 2),
            ..VideoState::default()
        },
    );
    dav.write("live-other", other.encode().unwrap()).unwrap();
    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[network]\ndevice_id = \"live-mine\"\n\n[player]\nmpv_options = {{ ao = \"null\" }}\n\n\
         [sync]\nurl = \"{url}\"\nuser = \"{user}\"\npassword = \"{password}\"\nautomatic = true\n"
    ));
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    until(Duration::from_secs(20), "this device's file", || {
        dav.devices()
            .ok()?
            .iter()
            .any(|d| d == "live-mine")
            .then_some(())
    });
    h.open(&fixture("av_basic.mkv"));
    h.wait("first-frame");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(40)).success());
    let mine = DeviceState::decode(&dav.read("live-mine").unwrap()).unwrap();
    assert_eq!(mine.videos["video/av_basic"].position_ms, 3000, "{mine:?}");
    assert!(!desk.log().contains(&password), "the password in the log");
}

// T-BOK-009: a book in vertical writing, and the word looked up on it (T-BOK-006)
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_book_reads_vertically() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[books]\nvertical = true\n");
    let path = desk.path("book.epub");
    nikomiru_books::sample::write(&path).unwrap();
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.cmd(json!({ "cmd": "books", "action": "open" }));
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the book", |s| {
        s["books"]["open"] == "見本の本" && s["books"]["vertical"] == true
    });
    // 今日は林檎を食べました: 食 is byte 18 of its paragraph
    h.cmd(json!({ "cmd": "books", "action": "click", "paragraph": 1, "offset": 18 }));
    h.wait("popup-rendered");
    let state = h.state_until("the word marked", |s| s["books"]["highlight"].is_array());
    assert_eq!(state["books"]["highlight"][0], 1);
    assert_eq!(state["books"]["highlight"][1], 18);
    if let Some(dir) = std::env::var_os("NIKOMIRU_SCREENSHOTS") {
        std::thread::sleep(Duration::from_millis(400));
        desk.capture(&std::path::Path::new(&dir).join("book-vertical.png"));
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
