// SPDX-License-Identifier: GPL-3.0-only

//! PLY-023: web video, separate audio, text subtitles, cards and source links
//! through a fake yt-dlp and local HTTP media, in the real app.
#![cfg(unix)]

use crate::support::{Desktop, exit_status, fixture};
use nikomiru_anki::{Client, fake::FakeAnki, lapis};
use nikomiru_online::fake_files::FakeFiles;
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, time::Duration};

fn extractor(desk: &Desktop, host: &FakeFiles, separate: bool) -> String {
    let path = desk.path("fake-yt-dlp");
    std::fs::write(
        &path,
        include_str!("../../crates/nikomiru-app/tests/support/fake_yt_dlp.py"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut metadata = json!({"title":"日本語の動画","url":host.url("/video"),"protocol":"http","subtitles":{"ja":[{"url":host.url("/ja.vtt"),"ext":"vtt"}]}});
    if separate {
        metadata["requested_formats"] = json!([
            {"url":host.url("/video"),"protocol":"http","vcodec":"mpeg4","acodec":"none"},
            {"url":host.url("/audio"),"protocol":"http","vcodec":"none","acodec":"aac"},
        ]);
    }
    std::fs::write(path.with_extension("json"), metadata.to_string()).unwrap();
    path.to_string_lossy().into()
}

// T-PLY-023, T-ANK-050
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon, ffmpeg and cargo xtask fixtures"]
fn a_web_video_has_subtitles_separate_audio_cards_and_a_persistent_scene_link() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let host = FakeFiles::start().unwrap();
    host.put("/video", std::fs::read(fixture("av_noaudio.mp4")).unwrap());
    let audio = desk.path("audio.m4a");
    assert!(
        std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-i",
                &fixture("av_basic.mkv"),
                "-map",
                "0:a:0",
                "-c:a",
                "aac",
                "-t",
                "5",
                audio.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    host.put("/audio", std::fs::read(&audio).unwrap());
    host.put(
        "/ja.vtt",
        "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\n食べました\n"
            .as_bytes()
            .to_vec(),
    );
    let program = extractor(&desk, &host, true);
    let fake = FakeAnki::start().unwrap();
    lapis::create(&Client::new(&fake.url(), "").unwrap()).unwrap();
    desk.write_config(&format!("[player]\nmpv_options = {{ ao = \"null\" }}\n[paths]\nyt_dlp = {program:?}\n[anki]\nurl = {:?}\n",fake.url()));
    let page = host.url("/watch?v=one");
    let (mut app, mut h) = desk.launch_with(&["--", &page]);
    h.wait("file-loaded");
    h.wait("first-frame");
    let s = h.state_until("downloaded text subtitles", |s| {
        s["subtitles"]["cues"] == 1 && s["aid"].as_i64().is_some_and(|id| id > 0)
    });
    assert_eq!(s["title"], "日本語の動画");
    assert_eq!(s["web_video"]["page"], page);
    let path = h.cmd(json!({"cmd":"mpv_property","name":"path"}));
    assert!(!path.as_str().unwrap().contains("/watch"));
    h.cmd(json!({"cmd":"click_grapheme","cue_index":0,"grapheme_index":0}));
    h.wait("popup-rendered");
    h.state_until("card button", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    h.wait("card-created");
    {
        let collection = fake.collection();
        assert_eq!(collection.notes.len(), 1);
        assert_eq!(
            collection.media.len(),
            2,
            "media names: {:?}\n{}",
            collection.media.keys().collect::<Vec<_>>(),
            desk.log()
                .lines()
                .filter(|l| l.contains("frame") || l.contains("image"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let uid = nikomiru_online::web_video::uid(&page);
    let link = nikomiru_anki::link::SourceLink {
        uid: uid.clone(),
        episode: 0,
        start_ms: 1000,
        end_ms: 4000,
        source_id: nikomiru_anki::link::new_source_id(),
    };
    h.open(&link.to_url());
    h.wait("file-loaded");
    let linked = h.state_until("the linked scene", |s| {
        s["paused"] == true
            && s["time_pos"]
                .as_f64()
                .is_some_and(|p| (p - 1.0).abs() < 0.2)
    });
    assert_eq!(linked["web_video"]["page"], page);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let lib = nikomiru_library::Library::open(&desk.data_file("library.redb")).unwrap();
    assert_eq!(lib.video(&uid).unwrap().unwrap().path, page);
    drop(lib);
    // A second start opens the persisted original address and resolves fresh
    // relays; expiring media URLs never become the library's source.
    let (mut app, mut h) = desk.launch_with(&["--", &link.to_url()]);
    h.wait("file-loaded");
    h.state_until("the linked web video after restart", |s| {
        s["web_video"]["page"] == page && s["paused"] == true
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PLY-023
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn missing_yt_dlp_and_cancelled_web_opens_leave_local_playback_usable() {
    let mut desk = Desktop::start();
    desk.write_config(
        "[player]\nmpv_options = { ao = \"null\" }\n[paths]\nyt_dlp = \"/nonexistent/yt-dlp\"\n",
    );
    let (mut app, mut h) = desk.launch();
    h.drop_files(&["https://example.org/watch?v=one"]);
    h.state_until("missing yt-dlp", |s| {
        s["web_video"]["error"]
            .as_str()
            .is_some_and(|e| e.contains("yt-dlp"))
    });
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("first-frame");
    assert!(h.state()["web_video"]["error"].is_null());
    let host = FakeFiles::start().unwrap();
    let program = extractor(&desk, &host, false);
    h.cmd(json!({"cmd":"settings","tab":"advanced","save":format!("[paths]\nyt_dlp = {program:?}\n")}));
    h.cmd(json!({"cmd":"settings","tab":"close"}));
    h.open(&host.url("/slow"));
    h.state_until("a pending web video", |s| s["web_video"]["loading"] == true);
    h.open(&fixture("av_noaudio.mp4"));
    h.wait("file-loaded");
    let local = h.state_until("local playback after cancellation", |s| {
        s["title"] == "av_noaudio.mp4" && s["web_video"]["loading"] == false
    });
    assert!(local["web_video"]["page"].is_null());
    assert!(host.requests().is_empty());
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
