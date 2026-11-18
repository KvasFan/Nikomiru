// SPDX-License-Identifier: GPL-3.0-only

//! MNG-003…MNG-005 with real pointer input: a box dragged over a CBZ page is
//! read and its first word looked up at once; a Mokuro volume's text is
//! looked up with a click on the page itself.
use super::support::{Desktop, exit_status};
use serde_json::json;
use std::io::Write;
use std::time::Duration;

fn page_png(width: u32, height: u32) -> Vec<u8> {
    let mut page = Vec::new();
    image::RgbImage::from_pixel(width, height, image::Rgb([230, 230, 230]))
        .write_to(
            &mut std::io::Cursor::new(&mut page),
            image::ImageFormat::Png,
        )
        .unwrap();
    page
}

// T-MNG-003, T-MNG-005: drag, read, and the dictionary at once
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_box_dragged_on_a_cbz_page_is_read_and_looked_up() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let path = desk.path("見本.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&page_png(600, 800)).unwrap();
    zip.finish().unwrap();
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.cmd(json!({ "cmd": "books", "action": "open" }));
    h.state_until("the model", |s| s["books"]["ocr_installed"] == true);
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the manga", |s| s["books"]["open"] == "見本");
    std::thread::sleep(Duration::from_millis(300));
    // a drag over the middle of the page, as a mouse makes it
    h.cmd(json!({ "cmd": "pointer", "x": 560.0, "y": 380.0 }));
    h.cmd(json!({ "cmd": "button", "x": 560.0, "y": 380.0, "pressed": true }));
    for (x, y) in [(580.0, 400.0), (620.0, 440.0), (680.0, 480.0)] {
        h.cmd(json!({ "cmd": "pointer", "x": x, "y": y }));
    }
    h.cmd(json!({ "cmd": "button", "x": 680.0, "y": 480.0, "pressed": false }));
    h.state_until("the text", |s| {
        s["books"]["page_text"] == "明日も食べました"
    });
    // the first word's entry, with no other click
    let s = h.state_until("looked up", |s| {
        s["popups"]
            .as_array()
            .and_then(|p| p.first())
            .is_some_and(|p| p["query"].as_str().is_some_and(|q| q.starts_with("明日")))
    });
    assert_eq!(s["books"]["chapter"], 0, "the page did not turn: {s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-MNG-010: a Mokuro page's own text, a click on a word, the dictionary
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn a_word_of_a_mokuro_page_is_looked_up_with_one_click() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let root = desk.path("");
    let images = root.join("Volume");
    std::fs::create_dir_all(&images).unwrap();
    image::RgbImage::from_pixel(100, 200, image::Rgb([210, 220, 230]))
        .save(images.join("1.png"))
        .unwrap();
    let path = root.join("Volume.mokuro");
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"version":"0.2.1","title":"Manga","volume":"One","pages":[
            {"img_path":"1.png","img_width":100,"img_height":200,"blocks":[
                {"box":[10,10,80,90],"vertical":false,"lines":["明日も食べました"]}]}]}),
        )
        .unwrap(),
    )
    .unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    h.state_until("Mokuro imported", |s| s["books"]["open"] == "Manga · One");
    // 食, nine bytes in
    h.cmd(json!({"cmd":"books","action":"region-at","paragraph":0,"offset":9}));
    h.state_until("the word under the click", |s| {
        s["books"]["page_text"] == "明日も食べました"
            && s["popups"]
                .as_array()
                .and_then(|p| p.first())
                .is_some_and(|p| p["query"].as_str().is_some_and(|q| q.starts_with("食")))
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-MNG-011: the page shown and the next are read ahead; a word of either
// is looked up with one click, no box dragged
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn the_page_shown_and_the_next_are_read_ahead() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let path = desk.path("見本.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, shade) in [("p1.png", 230), ("p2.png", 200), ("p3.png", 170)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        let mut page = Vec::new();
        image::RgbImage::from_pixel(600, 800, image::Rgb([shade, shade, shade]))
            .write_to(
                &mut std::io::Cursor::new(&mut page),
                image::ImageFormat::Png,
            )
            .unwrap();
        zip.write_all(&page).unwrap();
    }
    zip.finish().unwrap();
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.cmd(json!({ "cmd": "books", "action": "open" }));
    h.state_until("the model", |s| s["books"]["ocr_installed"] == true);
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the manga", |s| s["books"]["open"] == "見本");
    // the first page and the second, not the third yet
    let s = h.state_until("read ahead", |s| {
        s["books"]["ahead"] == json!({"0": 1, "1": 1})
    });
    assert_eq!(s["books"]["page_text"], "", "nothing chosen yet: {s}");
    // 食, nine bytes in, with one click
    h.cmd(json!({"cmd":"books","action":"region-at","paragraph":0,"offset":9}));
    h.state_until("the word under the click", |s| {
        s["books"]["page_text"] == "明日も食べました"
            && s["popups"]
                .as_array()
                .and_then(|p| p.first())
                .is_some_and(|p| p["query"].as_str().is_some_and(|q| q.starts_with("食")))
    });
    // on the next page, the one after is read ahead in its turn
    h.cmd(json!({"cmd":"books","action":"next"}));
    h.state_until("the third page", |s| {
        s["books"]["chapter"] == 1 && s["books"]["ahead"]["2"] == 1
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-MNG-005: the real model reads a balloon drawn in a real Japanese font,
// and its first word opens in the dictionary (`NIKOMIRU_OCR_DIR`: an
// installed manga-ocr folder; ImageMagick and Noto CJK draw the page)
#[test]
#[ignore = "L7: needs the real OCR model, ImageMagick and Noto CJK"]
fn l7_the_real_model_reads_a_dragged_balloon() {
    let Some(model) = std::env::var_os("NIKOMIRU_OCR_DIR") else {
        panic!("set NIKOMIRU_OCR_DIR");
    };
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.set_env("NIKOMIRU_TEST_OCR", "");
    let installed = desk.path("data/nikomiru/models/manga-ocr");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&model, &installed).unwrap();
    let page = desk.path("page.png");
    let status = std::process::Command::new("magick")
        .args(["-size", "600x800", "xc:white", "-font"])
        .arg("/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc")
        .args(["-pointsize", "48", "-fill", "black", "-gravity", "center"])
        .args(["-annotate", "+0+0", "明日も食べました"])
        .arg(&page)
        .status()
        .unwrap();
    assert!(status.success());
    let path = desk.path("見本.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&std::fs::read(&page).unwrap()).unwrap();
    zip.finish().unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({ "cmd": "books", "action": "open" }));
    h.state_until("the model", |s| s["books"]["ocr_installed"] == true);
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the manga", |s| s["books"]["open"] == "見本");
    // the whole page: the text sits in its middle
    h.cmd(json!({ "cmd": "books", "action": "read" }));
    // the model runs unoptimised in a test build: minutes, not seconds
    let s = super::support::until(Duration::from_secs(600), "the real reading", || {
        let s = h.state();
        (s["books"]["page_text"]
            .as_str()
            .is_some_and(|t| !t.is_empty())
            || !s["books"]["status"].is_null())
        .then_some(s)
    });
    assert!(
        s["books"]["status"].is_null(),
        "{}\n{}",
        s["books"],
        desk.log()
    );
    let text = s["books"]["page_text"].as_str().unwrap().to_string();
    assert!(text.contains("食べ"), "{text}");
    h.state_until("looked up", |s| {
        s["popups"].as_array().is_some_and(|p| !p.is_empty())
    });
    eprintln!("read: {text}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-MNG-011: the real detector finds the balloons of a page drawn in a real
// Japanese font, and the real reader reads them ahead (`NIKOMIRU_OCR_DIR`:
// an installed folder with the detector too)
#[test]
#[ignore = "L7: needs the real OCR models, ImageMagick and Noto CJK"]
fn l7_the_real_models_read_a_page_ahead() {
    let Some(model) = std::env::var_os("NIKOMIRU_OCR_DIR") else {
        panic!("set NIKOMIRU_OCR_DIR");
    };
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.set_env("NIKOMIRU_TEST_OCR", "");
    let installed = desk.path("data/nikomiru/models/manga-ocr");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&model, &installed).unwrap();
    let page = desk.path("page.png");
    // two balloons, each a column of vertical text
    let status = std::process::Command::new("magick")
        .args(["-size", "1200x1700", "xc:white", "-stroke", "black"])
        .args(["-strokewidth", "4", "-fill", "white"])
        .args(["-draw", "ellipse 900,300 170,240 0,360"])
        .args(["-draw", "ellipse 300,1200 200,260 0,360", "-stroke", "none"])
        .args(["-fill", "black", "-font"])
        .arg("/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc")
        .args(["-pointsize", "46", "-interline-spacing", "-6"])
        .args(["-annotate", "+900+130", "今\n日\nは\n晴\nれ\nで\nす"])
        .args(["-annotate", "+290+1010", "明\n日\nも\n食\nべ\nま\nし\nた"])
        .arg(&page)
        .status()
        .unwrap();
    assert!(status.success());
    let path = desk.path("見本.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&std::fs::read(&page).unwrap()).unwrap();
    zip.finish().unwrap();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({ "cmd": "books", "action": "open" }));
    h.state_until("the model", |s| s["books"]["ocr_installed"] == true);
    h.cmd(json!({ "cmd": "books", "action": "add", "path": path.to_string_lossy() }));
    h.state_until("the manga", |s| s["books"]["open"] == "見本");
    // the models run unoptimised in a test build: minutes, not seconds
    let s = super::support::until(Duration::from_secs(900), "the page read ahead", || {
        let s = h.state();
        s["books"]["ahead"]["0"].is_number().then_some(s)
    });
    assert_eq!(
        s["books"]["ahead"]["0"],
        2,
        "{}\n{}",
        s["books"],
        desk.log()
    );
    // for review by eye (MNG-012): the text written over the balloons
    let shots = std::env::var_os("NIKOMIRU_SCREENSHOTS").map(std::path::PathBuf::from);
    let shot = |name: &str| {
        if let Some(dir) = &shots {
            std::thread::sleep(Duration::from_millis(500));
            desk.capture(&dir.join(name));
        }
    };
    shot("manga-text.png");
    // the second balloon read, its 食 clicked
    h.cmd(json!({"cmd":"books","action":"region-at","paragraph":1,"offset":9}));
    let s = h.state_until("looked up", |s| {
        s["popups"].as_array().is_some_and(|p| !p.is_empty())
    });
    let text = s["books"]["page_text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(text.contains("食べ"), "{text}");
    shot("manga-lookup.png");
    eprintln!("read ahead: {text}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
