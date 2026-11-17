// SPDX-License-Identifier: GPL-3.0-only

//! M114: an MDict with its `.mdd` imported, looked up and mined in the actual app.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::time::Duration;

/// A PNG of 2×2 pixels.
fn png() -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

// T-DIC-036…T-DIC-038: rich definitions with an image and a sound from the
// `.mdd`, hostile markup inert, the sound played on click, and a card with both.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn an_mdict_with_resources_is_rich_in_the_popup_and_the_card() {
    use nikomiru_anki::fake::FakeAnki;
    let anki = FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&anki.url(), "").unwrap()).unwrap();
    let mut desk = Desktop::start();
    let dir = desk.path("mdict");
    std::fs::create_dir_all(&dir).unwrap();
    let png = png();
    let mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x00rest-of-sound".to_vec();
    let mdx = nikomiru_dict::foreign::mdict_fixture::write_pair(
        &dir,
        "Rich",
        &[(
            "ねこ【猫】",
            r#"<b>cat</b><script>alert(1)</script><br><img src="\pic\cat.png" alt="a cat" onerror="x()"> <a href="sound://snd\neko.mp3">▶</a> <a href="entry://犬">犬</a>"#,
        )],
        &[
            ("\\pic\\cat.png", &png),
            ("\\snd\\neko.mp3", &mp3),
            ("\\unused.bin", &[0; 32]),
        ],
    );
    {
        use std::sync::Arc;
        let data = desk.path("data").join("nikomiru");
        let library =
            Arc::new(nikomiru_library::Library::open(&data.join("library.redb")).unwrap());
        let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(nikomiru_core::SystemClock::new());
        let manager =
            nikomiru_library::DictionaryManager::open(library, data.join("dicts"), clock, "ja")
                .unwrap();
        manager
            .import(&mdx, None, &nikomiru_core::Cancel::new(), |_| {})
            .unwrap();
    }
    desk.write_config(&format!("[anki]\nurl = \"{}\"\n", anki.url()));
    let book = desk.path("neko.txt");
    std::fs::write(&book, "猫が好きです。\n").unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":book}));
    h.state_until("reader", |s| s["books"]["open"] == "neko");
    h.cmd(json!({"cmd":"books","action":"click","paragraph":0,"offset":0}));
    h.wait("popup-rendered");
    let content = h.cmd(json!({"cmd":"popup_content","level":0}));
    let text = content.to_string();
    assert!(text.contains("\"path\":\"pic/cat.png\""), "{text}");
    assert!(text.contains("snd/neko.mp3"), "{text}");
    for bad in ["alert", "onerror", "script"] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
    assert!(
        h.state()["dictionary_sound"].is_null(),
        "nothing plays by itself"
    );
    h.cmd(json!({"cmd":"popup_sound","level":0}));
    assert_eq!(h.state()["dictionary_sound"], "snd/neko.mp3");
    h.state_until("checked", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    let created = h.cmd(json!({ "cmd": "wait", "event": "card-created", "timeout_ms": 30000 }));
    let note_id = created["note_id"].as_u64().unwrap();
    {
        let collection = anki.collection();
        let note = collection.notes.iter().find(|n| n.id == note_id).unwrap();
        let all: String = note.fields.iter().map(|(_, v)| v.as_str()).collect();
        assert!(all.contains("<img"), "{all}");
        assert!(
            all.contains("[sound:nikomiru_") && all.contains(".mp3]"),
            "{all}"
        );
        let names: Vec<_> = collection.media.keys().cloned().collect();
        assert!(names.iter().any(|n| n.ends_with(".png")), "{names:?}");
        assert!(names.iter().any(|n| n.ends_with(".mp3")), "{names:?}");
        assert!(!all.contains("alert"));
    }
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
