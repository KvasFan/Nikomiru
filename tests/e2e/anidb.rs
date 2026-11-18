// SPDX-License-Identifier: GPL-3.0-only

//! MET-006: a series whose folder name says nothing, identified from one of
//! its files through a fake AniDB, then found on a fake AniList by AniDB's
//! title; a file AniDB does not know is reported.
use super::support::{Desktop, exit_status, fixture};
use nikomiru_online::anidb;
use nikomiru_online::fake::{FakeOnline, FakeSeries};
use nikomiru_online::fake_anidb::{CLIENT, FakeAniDb, PASSWORD, USER};
use serde_json::json;
use std::time::Duration;

// T-MET-006, T-PRV-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_series_is_identified_from_its_file_through_anidb() {
    let online = FakeOnline::start().unwrap();
    online.data().series = vec![FakeSeries {
        anilist_id: 154_587,
        mal_id: 52_991,
        romaji: "Sousou no Frieren".into(),
        english: Some("Frieren: Beyond Journey's End".into()),
        native: Some("葬送のフリーレン".into()),
        episodes: Some(28),
    }];
    let fake = FakeAniDb::start().unwrap();
    let video = fixture("av_basic.mkv");
    let bytes = std::fs::read(&video).unwrap();
    let (hash, _) = anidb::ed2k(&mut &bytes[..], bytes.len() as u64, &|| false).unwrap();
    fake.data().files.insert(
        (bytes.len() as u64, hash.clone()),
        "7|17617|271717|28|2023-2024|Sousou no Frieren|葬送のフリーレン|Frieren: Beyond Journey`s End|1|The Journey`s End".into(),
    );

    let mut desk = Desktop::start();
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n\
         [online]\nanilist_url = \"{}\"\nanidb_user = \"{USER}\"\nanidb_password = \"{}\"\n\
         anidb_client = \"{CLIENT}\"\nanidb_client_version = 1\nanidb_address = \"{}\"\n",
        online.anilist_url(),
        PASSWORD.replace('\\', "\\\\"),
        fake.address()
    ));
    desk.set_env("NIKOMIRU_TEST_ANIDB_INTERVAL_MS", "20");
    let root = desk.path("videos");
    let known = root.join("[Group] Untitled/ep01.mkv");
    std::fs::create_dir_all(known.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&video, &known).unwrap();
    let unknown = root.join("Mystery/ep01.mkv");
    std::fs::create_dir_all(unknown.parent().unwrap()).unwrap();
    std::fs::write(&unknown, b"not a known video").unwrap();

    let (mut app, mut h) = desk.launch();
    h.state_until("the page", |s| s["library_page"]["series"].is_array());
    h.cmd(json!({ "cmd": "library", "action": "import", "path": root }));
    let s = h.state_until("two series", |s| {
        s["library_page"]["series"]
            .as_array()
            .is_some_and(|a| a.len() == 2)
    });
    let id_of = |title: &str| {
        s["library_page"]["series"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["title"].as_str().is_some_and(|t| t.contains(title)))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    let (untitled, mystery) = (id_of("Untitled"), id_of("Mystery"));
    // nothing goes to AniDB by itself
    assert!(fake.data().packets.is_empty());

    h.cmd(json!({ "cmd": "library", "action": "series", "id": untitled }));
    h.state_until("its episodes", |s| {
        s["library_page"]["open_series"]["episodes"].is_array()
    });
    h.cmd(json!({ "cmd": "library", "action": "identify-file", "id": untitled }));
    let s = h.state_until("identified through AniDB", |s| {
        s["library_page"]["series_info"]["anilist_id"] == 154_587
    });
    assert_eq!(s["library_page"]["series_info"]["episodes"], 28, "{s}");
    let packets = fake.data().packets.clone();
    assert!(
        packets[0].starts_with(&format!("AUTH user={USER}&")),
        "{packets:?}"
    );
    assert!(
        packets[1].starts_with(&format!("FILE size={}&ed2k={hash}&", bytes.len())),
        "{packets:?}"
    );
    assert!(packets[2].starts_with("LOGOUT "), "{packets:?}");
    // AniList was asked AniDB's title, not the folder's
    let asked: Vec<String> = online.requests().iter().map(|r| r.body.clone()).collect();
    assert!(
        asked.iter().any(|b| b.contains("Sousou no Frieren")),
        "{asked:?}"
    );
    assert!(!asked.iter().any(|b| b.contains("Untitled")), "{asked:?}");

    // a file AniDB does not know: said so, nothing identified
    h.cmd(json!({ "cmd": "library", "action": "series", "id": mystery }));
    h.state_until("its episodes", |s| {
        s["library_page"]["open_series"]["id"] == mystery
            && s["library_page"]["open_series"]["episodes"].is_array()
    });
    h.cmd(json!({ "cmd": "library", "action": "identify-file", "id": mystery }));
    let s = h.state_until("the refusal", |s| {
        s["library_page"]["status"]["text"]
            .as_str()
            .is_some_and(|t| t.contains("AniDB"))
    });
    assert_eq!(
        s["library_page"]["series_info"],
        serde_json::Value::Null,
        "{s}"
    );
    assert!(s["library_page"]["identify"].is_null(), "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
