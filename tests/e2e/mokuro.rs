// SPDX-License-Identifier: GPL-3.0-only

//! Imported manga text remains local and supplies normal lookup/mining without OCR.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::{path::Path, time::Duration};
fn volume(root: &Path) -> std::path::PathBuf {
    let path = root.join("Volume.mokuro");
    let images = root.join("Volume");
    std::fs::create_dir(&images).unwrap();
    for name in ["1.png", "2.png"] {
        image::RgbImage::from_pixel(100, 200, image::Rgb([210, 220, 230]))
            .save(images.join(name))
            .unwrap();
    }
    std::fs::write(&path,serde_json::to_vec(&json!({"version":"0.2.1","title":"Manga","volume":"One","pages":[{"img_path":"1.png","img_width":100,"img_height":200,"blocks":[{"box":[10,10,80,90],"lines":["猫です"]}]},{"img_path":"2.png","img_width":100,"img_height":200,"blocks":[{"box":[10,10,80,90],"lines":["食べる"]}]}]})).unwrap()).unwrap();
    path
}
// T-MNG-007, T-MNG-008: real imported text, JPEG cropping and Anki media use the existing manga pipeline.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn imported_mokuro_regions_are_read_mined_and_reopened_without_ocr() {
    let anki = nikomiru_anki::fake::FakeAnki::start().unwrap();
    nikomiru_anki::lapis::create(&nikomiru_anki::Client::new(&anki.url(), "").unwrap()).unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(&format!("[anki]\nurl=\"{}\"\n", anki.url()));
    let path = volume(&desk.path(""));
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    let first = h.state_until("Mokuro imported", |s| s["books"]["open"] == "Manga · One");
    let uid = first["books"]["uid"].clone();
    assert!(uid.as_str().unwrap().starts_with("mokuro:"));
    assert_eq!(first["books"]["region_counts"]["0"], 1);
    h.cmd(json!({"cmd":"books","action":"region","paragraph":0,"offset":0}));
    h.state_until("stored text and crop", |s| {
        s["books"]["page_text"] == "猫です"
            && s["books"]["crop_bytes"].as_u64().is_some_and(|n| n > 0)
    });
    h.cmd(json!({"cmd":"books","action":"click","paragraph":0,"offset":0}));
    h.wait("popup-rendered");
    h.state_until("card ready", |s| s["popups"][0]["cards"][0] == "add");
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    h.wait("card-created");
    {
        let collection = anki.collection();
        assert!(!collection.media.is_empty());
        assert!(collection.notes.iter().any(|n| {
            n.fields
                .iter()
                .any(|(name, s)| name == "Sentence" && s.contains("猫") && s.contains("です"))
        }));
    }
    h.key("Escape");
    h.cmd(json!({"cmd":"books","action":"next"}));
    h.state_until("second original page", |s| s["books"]["chapter"] == 1);
    h.cmd(json!({"cmd":"books","action":"region","paragraph":1,"offset":0}));
    h.state_until("second stored region", |s| {
        s["books"]["page_text"] == "食べる"
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":uid}));
    h.state_until("original page position retained", |s| {
        s["books"]["uid"] == uid && s["books"]["chapter"] == 1
    });
    h.cmd(json!({"cmd":"books","action":"region","paragraph":1,"offset":0}));
    h.state_until("region reread without recognition", |s| {
        s["books"]["page_text"] == "食べる"
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
// T-MNG-009: malformed volumes never replace a currently valid reader.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn invalid_mokuro_keeps_the_active_volume_and_its_progress() {
    let mut desk = Desktop::start();
    let path = volume(&desk.path(""));
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    let first = h.state_until("valid source", |s| s["books"]["open"] == "Manga · One");
    let bad = desk.path("bad.mokuro");
    let mut data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    data["pages"][0]["img_path"] = "../outside.png".into();
    std::fs::write(&bad, serde_json::to_vec(&data).unwrap()).unwrap();
    h.cmd(json!({"cmd":"books","action":"add","path":bad}));
    h.state_until("invalid volume reported", |s| {
        s["books"]["status"].is_string()
    });
    assert_eq!(h.state()["books"]["uid"], first["books"]["uid"]);
    assert_eq!(h.state()["books"]["chapter"], first["books"]["chapter"]);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
