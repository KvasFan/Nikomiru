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
