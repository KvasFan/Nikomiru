// SPDX-License-Identifier: GPL-3.0-only

//! L4: the media servers against fake ones (TEST §6.13, M11).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use nikomiru_online::OnlineError;
use nikomiru_online::fake_servers::{Data, FakeItem, FakeServers, PASSWORD, TOKEN, USER};
use nikomiru_online::http::Http;
use nikomiru_online::servers::jellyfin::Jellyfin;
use nikomiru_online::servers::plex::{Plex, PlexTv};
use nikomiru_online::servers::relay::{MAX_CONNECTIONS, Relay};
use nikomiru_online::servers::{
    ItemKind, MediaServer, ParentKind, PlayState, Stream, checked_server_url,
};
use reqwest::Url;

fn data() -> Data {
    let mut items = vec![FakeItem {
        id: "series".into(),
        kind: "Series".into(),
        title: "A Series".into(),
        parent: "lib".into(),
        ..FakeItem::default()
    }];
    for n in 1..=250 {
        items.push(FakeItem {
            id: format!("e{n}"),
            kind: "Episode".into(),
            title: format!("Episode {n}"),
            parent: "series".into(),
            index: n,
            position_ms: 5000,
            last_played_s: Some(100),
        });
    }
    Data {
        libraries: vec![("lib".into(), "Anime".into())],
        items,
        file: (0..=255u8).cycle().take(1 << 20).collect(),
        subtitle: "1\n00:00:01,000 --> 00:00:02,000\n字幕\n".into(),
        ..Data::default()
    }
}

fn url(text: &str) -> Url {
    checked_server_url(text, false).unwrap()
}

fn jellyfin(fake: &FakeServers) -> Jellyfin {
    let http = Http::new("").unwrap();
    Jellyfin::login(http, url(&fake.jellyfin_url()), USER, PASSWORD, "device").unwrap()
}

fn plex(fake: &FakeServers) -> Plex {
    Plex::new(
        Http::new("").unwrap(),
        url(&fake.plex_url()),
        TOKEN,
        "device",
    )
}

// T-SRV-002
#[test]
fn jellyfin_logs_in_and_keeps_only_the_token() {
    let fake = FakeServers::start(data()).unwrap();
    let server = jellyfin(&fake);
    assert_eq!(server.token(), TOKEN);
    assert_eq!(server.user_id(), "user-1");
    let shown = format!("{server:?}");
    assert!(
        !shown.contains(TOKEN) && !shown.contains(PASSWORD),
        "{shown}"
    );
    let login = &fake.requests()[0];
    let header = login.header("authorization").unwrap();
    for part in ["Client=\"Nikomiru\"", "DeviceId=\"device\"", "Version=\""] {
        assert!(header.contains(part), "{header}");
    }
    let wrong = Jellyfin::login(
        Http::new("").unwrap(),
        url(&fake.jellyfin_url()),
        USER,
        "wrong",
        "device",
    );
    assert!(matches!(wrong, Err(OnlineError::Unauthorized { .. })));
}

// T-SRV-002: an expired token
#[test]
fn a_refused_token_is_reported() {
    let fake = FakeServers::start(data()).unwrap();
    let http = Http::new("").unwrap();
    let mut server = Jellyfin::new(http, url(&fake.jellyfin_url()), "old", "user-1", "device");
    assert!(matches!(
        server.libraries(),
        Err(OnlineError::Unauthorized { .. })
    ));
}

// T-SRV-003
#[test]
fn plex_signs_in_with_a_pin() {
    let fake = FakeServers::start(Data {
        pin_polls: 2,
        ..data()
    })
    .unwrap();
    let tv = PlexTv::new(Http::new("").unwrap(), url(&fake.plex_tv_url()), "device");
    let pin = tv.pin().unwrap();
    let page = tv.sign_in_url(&pin);
    assert!(page.starts_with("https://app.plex.tv/auth#?"), "{page}");
    assert!(
        page.contains("code=abcd") && page.contains("clientID=device"),
        "{page}"
    );
    assert_eq!(tv.token(&pin).unwrap(), None);
    assert_eq!(tv.token(&pin).unwrap(), None);
    assert_eq!(tv.token(&pin).unwrap().as_deref(), Some(TOKEN));
    let servers = tv.resources(TOKEN).unwrap();
    assert_eq!(servers.len(), 1, "players are left out");
    assert_eq!(servers[0].name, "Home");
    assert_eq!(servers[0].token, TOKEN);
    assert_eq!(
        servers[0].addresses,
        [
            "http://192.168.1.2:32400",
            "https://remote.example.org",
            "https://relay.example.org"
        ]
    );
    assert!(!format!("{:?}", servers[0]).contains(TOKEN));
}

fn every_page(server: &mut dyn MediaServer, parent: &str, kind: ParentKind) -> Vec<String> {
    let mut ids = Vec::new();
    loop {
        let page = server.children(parent, ids.len(), kind).unwrap();
        assert!(page.items.len() <= 100);
        ids.extend(page.items.iter().map(|i| i.id.clone()));
        if ids.len() >= page.total || page.items.is_empty() {
            return ids;
        }
    }
}

fn browse(server: &mut dyn MediaServer) {
    let libraries = server.libraries().unwrap();
    assert_eq!(libraries.len(), 1, "only series and movies");
    assert_eq!(libraries[0].title, "Anime");
    let series = server.children("lib", 0, ParentKind::Library).unwrap();
    assert_eq!(series.items[0].kind, ItemKind::Series);
    assert_eq!(every_page(server, "series", ParentKind::Item).len(), 250);
    let found = server.search("lib", "Episode 12", 0).unwrap();
    assert_eq!(found.total, 11, "12 and 120…129");
    let item = server.item("e3").unwrap();
    assert_eq!(item.kind, ItemKind::Episode);
    assert_eq!((item.season, item.episode), (Some(1), Some(3)));
    assert_eq!(item.position_ms, 5000);
    assert_eq!(item.duration_ms, 60_000);
    assert_eq!(item.series.as_deref(), Some("A Series"));
    assert_eq!(server.cover(&item.cover.unwrap()).unwrap(), b"cover");
    assert!(matches!(
        server.item("nothing"),
        Err(OnlineError::Gone { .. })
    ));
}

// T-SRV-006, T-SRV-013
#[test]
fn jellyfin_lists_its_library() {
    let fake = FakeServers::start(data()).unwrap();
    browse(&mut jellyfin(&fake));
    let item = jellyfin(&fake).item("e3").unwrap();
    assert_eq!(item.anilist_id, Some(1234));
    assert_eq!(item.last_played_s, Some(1_793_437_200));
}

// T-SRV-006, T-SRV-013
#[test]
fn plex_lists_its_library() {
    let fake = FakeServers::start(data()).unwrap();
    browse(&mut plex(&fake));
}

fn play(server: &mut dyn MediaServer, fake: &FakeServers) {
    let playable = server.playable("e3").unwrap();
    assert_eq!(playable.subtitles.len(), 1, "only external text subtitles");
    assert_eq!(playable.subtitles[0].format, "srt");
    assert_eq!(playable.subtitles[0].language.as_deref(), Some("jpn"));
    assert_eq!(
        server.subtitle(&playable.subtitles[0]).unwrap(),
        fake.data().subtitle
    );
    for state in [
        PlayState::Started,
        PlayState::Playing,
        PlayState::Paused,
        PlayState::Stopped,
    ] {
        server
            .report("e3", &playable.source, state, 1000, 60_000)
            .unwrap();
    }
    server.mark_played("e3").unwrap();
    server.logout().unwrap();
}

// T-SRV-010, T-SRV-011
#[test]
fn jellyfin_plays_and_reports() {
    let fake = FakeServers::start(data()).unwrap();
    play(&mut jellyfin(&fake), &fake);
    let requests = fake.requests();
    let progress = requests
        .iter()
        .find(|r| r.target.ends_with("/Sessions/Playing/Progress"))
        .unwrap();
    assert!(
        progress.body.contains("\"PositionTicks\":10000000"),
        "{}",
        progress.body
    );
    assert!(progress.body.contains("\"PlayMethod\":\"DirectPlay\""));
    assert!(
        requests
            .iter()
            .any(|r| r.target.contains("/Sessions/Playing/Stopped"))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.target.contains("/UserPlayedItems/e3"))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.target.ends_with("/Sessions/Logout"))
    );
}

// T-SRV-010, T-SRV-011
#[test]
fn plex_plays_and_reports() {
    let fake = FakeServers::start(data()).unwrap();
    play(&mut plex(&fake), &fake);
    let requests = fake.requests();
    let reported = |state: &str| {
        requests
            .iter()
            .any(|r| r.target.contains("timeline") && r.target.contains(state))
    };
    assert!(reported("state=playing&time=1000&duration=60000"));
    assert!(reported("state=paused") && reported("state=stopped"));
    assert!(
        requests
            .iter()
            .any(|r| r.target.contains("scrobble?key=e3"))
    );
}

fn exchange(address: &str, request: &str) -> Vec<u8> {
    let mut connection = TcpStream::connect(address).unwrap();
    connection.write_all(request.as_bytes()).unwrap();
    let mut out = Vec::new();
    connection.read_to_end(&mut out).unwrap();
    out
}

fn body(answer: &[u8]) -> &[u8] {
    let i = answer.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    &answer[i + 4..]
}

fn split(relay: &Relay) -> (String, String) {
    let u = Url::parse(&relay.url()).unwrap();
    (
        format!("127.0.0.1:{}", u.port().unwrap()),
        u.path().to_string(),
    )
}

fn stream_of(server: &mut dyn MediaServer) -> Stream {
    server.playable("e3").unwrap().stream
}

// T-SRV-009, T-SRV-004: the token is added by the relay, and only there
#[test]
fn the_relay_serves_ranges_with_the_token() {
    let fake = FakeServers::start(data()).unwrap();
    for stream in [stream_of(&mut jellyfin(&fake)), stream_of(&mut plex(&fake))] {
        assert!(!format!("{stream:?}").contains(TOKEN));
        assert!(!stream.url.as_str().contains(TOKEN));
        let relay = Relay::start(stream, "").unwrap();
        assert!(!relay.url().contains(TOKEN));
        let (address, path) = split(&relay);
        let answer = exchange(
            &address,
            &format!("GET {path} HTTP/1.1\r\nHost: x\r\nRange: bytes=256-511\r\n\r\n"),
        );
        let text = String::from_utf8_lossy(&answer);
        assert!(text.starts_with("HTTP/1.1 206"), "{text}");
        assert!(
            text.contains("content-range: bytes 256-511/1048576"),
            "{text}"
        );
        assert_eq!(body(&answer), &fake.data().file[256..512]);
        let head = exchange(&address, &format!("HEAD {path} HTTP/1.1\r\n\r\n"));
        assert!(String::from_utf8_lossy(&head).contains("content-length: 1048576"));
        assert!(body(&head).is_empty());
    }
}

// T-SRV-009
#[test]
fn the_relay_answers_nothing_else() {
    let fake = FakeServers::start(data()).unwrap();
    let relay = Relay::start(stream_of(&mut jellyfin(&fake)), "").unwrap();
    let (address, path) = split(&relay);
    let status = |request: String| {
        String::from_utf8_lossy(&exchange(&address, &request))
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    };
    assert!(status("GET /other HTTP/1.1\r\n\r\n".into()).contains("404"));
    assert!(status(format!("POST {path} HTTP/1.1\r\n\r\n")).contains("405"));
    assert!(status(format!("DELETE {path} HTTP/1.1\r\n\r\n")).contains("405"));
}

// T-SRV-004: a redirect would take the token elsewhere
#[test]
fn the_relay_does_not_follow_redirects() {
    let fake = FakeServers::start(data()).unwrap();
    let other = FakeServers::start(data()).unwrap();
    fake.data().redirect_to = Some(format!("{}Videos/e3/stream", other.jellyfin_url()));
    let relay = Relay::start(stream_of(&mut jellyfin(&fake)), "").unwrap();
    let (address, path) = split(&relay);
    let answer = exchange(&address, &format!("GET {path} HTTP/1.1\r\n\r\n"));
    let text = String::from_utf8_lossy(&answer);
    assert!(text.starts_with("HTTP/1.1 502"), "{text}");
    assert!(!text.contains("127.0.0.1"), "{text}");
    assert!(other.requests().is_empty());
}

// T-SRV-009
#[test]
fn the_relay_limits_connections_and_lets_go() {
    let fake = FakeServers::start(data()).unwrap();
    let relay = Relay::start(stream_of(&mut jellyfin(&fake)), "").unwrap();
    let (address, path) = split(&relay);
    // idle connections hold their place until they ask or time out
    let held: Vec<TcpStream> = (0..MAX_CONNECTIONS)
        .map(|_| TcpStream::connect(&address).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(100));
    let over = exchange(&address, &format!("GET {path} HTTP/1.1\r\n\r\n"));
    assert!(String::from_utf8_lossy(&over).starts_with("HTTP/1.1 503"));
    drop(held);
    let started = Instant::now();
    loop {
        let again = exchange(
            &address,
            &format!("GET {path} HTTP/1.1\r\nRange: bytes=0-1\r\n\r\n"),
        );
        if String::from_utf8_lossy(&again).starts_with("HTTP/1.1 206") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the places came back"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(relay);
    std::thread::sleep(Duration::from_millis(100));
    assert!(TcpStream::connect(&address).is_err(), "gone with the video");
}

// T-SRV-013
#[test]
fn a_server_that_dies_is_an_error() {
    let fake = FakeServers::start(data()).unwrap();
    let mut server = jellyfin(&fake);
    drop(fake);
    assert!(matches!(
        server.libraries(),
        Err(OnlineError::Unreachable { .. } | OnlineError::Timeout { .. })
    ));
}
