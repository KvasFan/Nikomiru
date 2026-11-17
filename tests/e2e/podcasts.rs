// SPDX-License-Identifier: GPL-3.0-only

//! M112: podcast feeds, resumed offline downloads, transcripts and timed text
//! in the actual app, against a fake host.
use super::support::{Desktop, exit_status, fixture};
use nikomiru_online::fake_files::FakeFiles;
use serde_json::json;
use std::time::Duration;

fn feed(host: &FakeFiles, second: bool) -> Vec<u8> {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:podcast="https://podcastindex.org/namespace/1.0">
<channel><title>日本語ポッド</title><link>{site}</link>
<item><title>第1回</title><guid>ep-1</guid><link>{site}1</link>
<enclosure url="{a}" type="audio/mp4" length="{len}"/>
<podcast:transcript url="{vtt}" type="text/vtt" language="ja"/></item>"#,
        site = host.url("/site/"),
        a = host.url("/ep1.m4a"),
        len = 0,
        vtt = host.url("/ep1.vtt"),
    );
    if second {
        xml.push_str(&format!(
            r#"<item><title>第2回</title><guid>ep-2</guid>
<enclosure url="{b}" type="audio/mp4"/>
<podcast:transcript url="{json}" type="application/json"/></item>"#,
            b = host.url("/ep2.m4a"),
            json = host.url("/ep2.json"),
        ));
    }
    xml.push_str("</channel></rss>");
    xml.into_bytes()
}

// T-POD-001…T-POD-004: explicit feed, interrupted and resumed download with its
// transcript, offline lookup/replay/card, unsupported transcript with imported LRC,
// restart, refresh keeping a download no longer listed, and explicit deletion.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon, ffmpeg and `cargo xtask fixtures`"]
fn podcast_episodes_download_resume_and_play_with_their_text() {
    use nikomiru_anki::fake::FakeAnki;
    let anki = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&anki.url(), "").unwrap()).unwrap();
    let host = FakeFiles::start().unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n[anki]\nurl = \"{}\"\n",
        anki.url()
    ));
    let audio = desk.path("made.m4a");
    let made = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
        .arg(fixture("av_basic.mkv"))
        .args(["-vn", "-sn", "-map", "0:a:0", "-c:a", "aac"])
        .arg(&audio)
        .status()
        .unwrap();
    assert!(made.success());
    let bytes = std::fs::read(&audio).unwrap();
    host.put("/ep1.m4a", bytes.clone());
    host.put("/ep2.m4a", bytes.clone());
    host.put(
        "/ep1.vtt",
        "WEBVTT\n\n00:00.200 --> 00:03.500\n食べました\n\n00:04.000 --> 00:06.000\n日本語\n"
            .as_bytes()
            .to_vec(),
    );
    host.put("/ep2.json", b"{\"segments\":[]}".to_vec());
    host.put("/feed.xml", feed(&host, true));
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"podcasts","action":"open"}));
    h.cmd(json!({"cmd":"podcasts","action":"add","url":"http://example.org/feed"}));
    assert!(
        h.state()["podcasts"]["error"].is_string(),
        "clear HTTP refused"
    );
    h.cmd(json!({"cmd":"podcasts","action":"add","url":host.url("/feed.xml")}));
    let added = h.state_until("feed added", |s| {
        s["podcasts"]["busy"] == false
            && s["podcasts"]["feeds"][0]["episodes"]
                .as_array()
                .is_some_and(|e| e.len() == 2)
    });
    assert_eq!(added["podcasts"]["feeds"][0]["title"], "日本語ポッド");
    assert!(
        host.requests().iter().all(|(p, _)| p == "/feed.xml"),
        "adding downloads nothing"
    );
    // an interrupted download keeps its partial file
    host.cut_after("/ep1.m4a", bytes.len() / 3);
    h.cmd(json!({"cmd":"podcasts","action":"download","feed":0,"episode":0}));
    h.state_until("interrupted", |s| {
        s["podcasts"]["download"].is_null()
            && s["podcasts"]["error"].is_string()
            && s["podcasts"]["feeds"][0]["episodes"][0]["partial"] == true
    });
    host.clear_requests();
    h.cmd(json!({"cmd":"podcasts","action":"download","feed":0,"episode":0}));
    let done = h.state_until("resumed and recorded", |s| {
        s["podcasts"]["busy"] == false
            && s["podcasts"]["feeds"][0]["episodes"][0]["file"].is_string()
    });
    let ranges: Vec<_> = host
        .requests()
        .into_iter()
        .filter(|(p, _)| p == "/ep1.m4a")
        .collect();
    assert_eq!(ranges.len(), 1);
    assert!(
        ranges[0]
            .1
            .as_deref()
            .is_some_and(|r| r.starts_with("bytes=") && r != "bytes=0-"),
        "{ranges:?}"
    );
    let episode = &done["podcasts"]["feeds"][0]["episodes"][0];
    assert!(
        episode["transcript_file"]
            .as_str()
            .is_some_and(|t| t.ends_with(".ja.vtt"))
    );
    let dir = std::path::PathBuf::from(done["podcasts"]["dir"].as_str().unwrap());
    let file = dir.join(episode["file"].as_str().unwrap());
    assert_eq!(
        std::fs::read(&file).unwrap(),
        bytes,
        "the whole file, resumed"
    );
    // offline playback with its transcript: lookup, line replay and a card
    h.cmd(json!({"cmd":"podcasts","action":"play","feed":0,"episode":0}));
    h.state_until("episode playing with its text", |s| {
        s["loaded"] == true && s["subtitles"]["cues"] == 2 && s["podcasts"]["open"] == false
    });
    h.wait("cue-shown");
    h.cmd(json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.cmd(json!({"cmd":"popup_button","button":"replay"}));
    h.state_until("the line again", |s| {
        s["time_pos"].as_f64().is_some_and(|t| t < 3.5)
    });
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({ "cmd": "key", "code": "Enter", "mods": ["Ctrl"] }));
    let created = h.cmd(json!({ "cmd": "wait", "event": "card-created", "timeout_ms": 30000 }));
    let note_id = created["note_id"].as_u64().unwrap();
    {
        let collection = anki.collection();
        let note = collection.notes.iter().find(|n| n.id == note_id).unwrap();
        assert!(
            note.fields
                .iter()
                .any(|(n, v)| n == "SentenceAudio" && v.contains("[sound:")),
            "{:?}",
            note.fields
        );
    }
    h.key("Escape");
    // an unsupported transcript is reported; own LRC text is used instead
    h.cmd(json!({"cmd":"podcasts","action":"open"}));
    h.cmd(json!({"cmd":"podcasts","action":"download","feed":0,"episode":1}));
    let second = h.state_until("second episode", |s| {
        s["podcasts"]["busy"] == false
            && s["podcasts"]["feeds"][0]["episodes"][1]["file"].is_string()
    });
    assert!(
        second["podcasts"]["notice"]
            .as_str()
            .is_some_and(|n| n.contains("application/json")),
        "{}",
        second["podcasts"]
    );
    assert!(second["podcasts"]["feeds"][0]["episodes"][1]["transcript_file"].is_null());
    let bad = desk.path("bad.lrc");
    std::fs::write(&bad, "no times here").unwrap();
    h.cmd(json!({"cmd":"podcasts","action":"import","feed":0,"episode":1,"path":bad}));
    h.state_until("invalid text refused", |s| {
        s["podcasts"]["busy"] == false && s["podcasts"]["error"].is_string()
    });
    let lrc = desk.path("lesson.lrc");
    std::fs::write(&lrc, "[00:00.20]食べました\n[00:04.00]日本語\n").unwrap();
    h.cmd(json!({"cmd":"podcasts","action":"import","feed":0,"episode":1,"path":lrc}));
    h.state_until("own timed text", |s| {
        s["podcasts"]["feeds"][0]["episodes"][1]["transcript_file"]
            .as_str()
            .is_some_and(|t| t.ends_with(".lrc"))
    });
    h.cmd(json!({"cmd":"podcasts","action":"play","feed":0,"episode":1}));
    h.state_until("lyrics loaded", |s| {
        s["loaded"] == true && s["subtitles"]["cues"] == 2
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    // restart: records stay; a refresh keeps the download no longer listed
    host.put("/feed.xml", feed(&host, false));
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"podcasts","action":"open"}));
    h.state_until("feeds after restart", |s| {
        s["podcasts"]["feeds"][0]["episodes"]
            .as_array()
            .is_some_and(|e| e.len() == 2)
    });
    h.cmd(json!({"cmd":"podcasts","action":"refresh","feed":0}));
    let refreshed = h.state_until("refreshed", |s| {
        s["podcasts"]["busy"] == false
            && s["podcasts"]["fetching"].is_null()
            && s["podcasts"]["feeds"][0]["episodes"][1]["listed"] == false
    });
    assert!(refreshed["podcasts"]["feeds"][0]["episodes"][1]["file"].is_string());
    h.cmd(json!({"cmd":"podcasts","action":"delete","feed":0,"episode":0}));
    h.state_until("download deleted", |s| {
        s["podcasts"]["busy"] == false && s["podcasts"]["feeds"][0]["episodes"][0]["file"].is_null()
    });
    assert!(!file.exists());
    h.cmd(json!({"cmd":"podcasts","action":"remove","feed":0}));
    h.state_until("feed removed", |s| {
        s["podcasts"]["busy"] == false
            && s["podcasts"]["feeds"].as_array().is_some_and(Vec::is_empty)
    });
    assert!(!file.parent().unwrap().exists(), "its files went with it");
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
