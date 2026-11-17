// SPDX-License-Identifier: GPL-3.0-only

//! M110: real document imports use native reader, dictionary, mining and source paths.
use super::support::{Desktop, Harness, exit_status};
use serde_json::{Value, json};
use std::{io::Write, path::Path, time::Duration};

fn archive(path: &Path, entries: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, text) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}
fn open(h: &mut Harness, path: &Path, title: &str) -> Value {
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"add","path":path}));
    h.state_until("imported document reader", |s| {
        s["books"]["open"] == title && s["books"]["busy"] == false
    })
}
fn mine(h: &mut Harness) {
    h.cmd(json!({"cmd":"books","action":"click","paragraph":0,"offset":0}));
    h.wait("popup-rendered");
    assert_eq!(h.state()["popups"][0]["query"], "猫です");
    h.cmd(json!({"cmd":"card","level":0,"entry":0}));
    h.wait("card-created");
    h.key("Escape");
}
fn index(h: &mut Harness, uid: &Value, chapters: &[u64]) {
    h.cmd(json!({"cmd":"sentence-bank","action":"open"}));
    h.cmd(json!({"cmd":"sentence-bank","action":"book"}));
    h.state_until("document sentence index committed", |s| {
        s["sentence_bank"]["indexing"] == false
            && s["sentence_bank"]["batches"]
                .as_array()
                .is_some_and(|b| b.len() == 1)
    });
    h.cmd(json!({"cmd":"sentence-bank","action":"query","expression":"猫","reading":"ねこ"}));
    let state = h.state_until("document exact examples", |s| {
        s["sentence_bank"]["querying"] == false
            && s["sentence_bank"]["expression"] == "猫"
            && s["sentence_bank"]["examples"]
                .as_array()
                .is_some_and(|e| e.len() == chapters.len())
    });
    let mut actual = Vec::new();
    for example in state["sentence_bank"]["examples"].as_array().unwrap() {
        assert_eq!(example["sentence"]["source"]["uid"], *uid);
        assert_eq!(example["sentence"]["source"]["paragraph"], 0);
        assert_eq!(example["sentence"]["text"], "猫です。");
        actual.push(example["sentence"]["source"]["chapter"].as_u64().unwrap());
    }
    actual.sort_unstable();
    assert_eq!(actual, chapters);
    h.cmd(json!({"cmd":"sentence-bank","action":"close"}));
}
fn analyse(h: &mut Harness, uid: &Value) {
    h.cmd(json!({"cmd":"books","action":"close"}));
    h.cmd(json!({"cmd":"books","action":"analyse","path":uid}));
    h.state_until("native whole-document analysis", |s| {
        s["books"]["list"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["uid"] == *uid && b["whole_score"]["words"].as_u64().is_some_and(|n| n > 0))
    });
}
fn document_flow(format: &str) {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[general]\nui_language=\"en\"\n[subtitles]\nword_colors=true\n[anki]\ndestination=\"nikomiru\"\n");
    let path = desk.path(&format!("Owned.{format}"));
    match format {
        "htmlz" => archive(&path, &[("index.html", &format!(
            "<script>never()</script><p><ruby>猫<rt>ねこ</rt></ruby>です。</p><p>食べました。</p>{}",
            "<p>続きです。</p>".repeat(60)
        ))]),
        "rtf" => std::fs::write(&path, format!(
            "{{\\rtf1\\ansi\\ansicpg932 \\'94\\'4c\\'82\\'c5\\'82\\'b7\\'81\\'42\\par \\u-26401?\\u12409?\\u12414?\\u12375?\\u12383?\\u12290?{}}}",
            "\\par \\u32154?\\u12365?\\u12391?\\u12377?\\u12290?".repeat(60)
        )).unwrap(),
        "docx" => archive(&path, &[
            ("[Content_Types].xml", r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#),
            ("word/document.xml", &format!(
                r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>猫です。</w:t></w:r></w:p><w:p><w:r><w:t>食べました。</w:t></w:r></w:p>{}</w:body></w:document>"#,
                "<w:p><w:r><w:t>続きです。</w:t></w:r></w:p>".repeat(60)
            )),
        ]),
        _ => unreachable!(),
    }
    let (mut app, mut h) = desk.launch();
    let state = open(&mut h, &path, "Owned");
    let uid = state["books"]["uid"].clone();
    assert!(uid.as_str().unwrap().starts_with("document:"));
    assert_eq!(state["books"]["paragraphs"][0]["text"], "猫です。");
    assert_eq!(state["books"]["paragraphs"][1]["text"], "食べました。");
    if format == "htmlz" {
        assert_eq!(state["books"]["paragraphs"][0]["ruby"][0]["text"], "ねこ");
    }
    mine(&mut h);
    index(&mut h, &uid, &[0]);
    let bad = desk.path(&format!("Bad.{format}"));
    std::fs::write(&bad, "malformed document").unwrap();
    h.cmd(json!({"cmd":"books","action":"add","path":bad}));
    h.state_until("invalid import retains current document", |s| {
        s["books"]["busy"] == false
            && s["books"]["status"]
                .as_str()
                .is_some_and(|t| t.contains("invalid"))
    });
    assert_eq!(h.state()["books"]["uid"], uid);
    h.cmd(json!({"cmd":"pointer","x":500.0,"y":400.0}));
    h.cmd(json!({"cmd":"wheel","y":1600.0}));
    h.state_until("document reading progress", |s| {
        s["books"]["paragraph"].as_u64().is_some_and(|p| p >= 15)
    });
    analyse(&mut h, &uid);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":uid}));
    h.state_until("document position survives restart", |s| {
        s["books"]["uid"] == uid && s["books"]["paragraph"].as_u64().is_some_and(|p| p >= 15)
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-DOC-001, T-DOC-005: inert HTMLZ/ruby, lookup/mining/index, invalid isolation, analysis and restart.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn htmlz_uses_the_complete_document_reader_flow() {
    document_flow("htmlz");
}

// T-DOC-002, T-DOC-005: actual native CP932 and Unicode RTF use the same reader/mining/analysis.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn rtf_uses_the_complete_document_reader_flow() {
    document_flow("rtf");
}

// T-DOC-003, T-DOC-005: a real DOCX body remains usable through source indexing and restart.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn docx_uses_the_complete_document_reader_flow() {
    document_flow("docx");
}

// T-DOC-004, T-DOC-005: installed Poppler, empty page indices, frozen phrase locations and restart.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and installed Poppler"]
fn pdf_preserves_original_pages_and_native_source_locations() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[general]\nui_language=\"en\"\n[subtitles]\nword_colors=true\n[anki]\ndestination=\"nikomiru\"\n");
    let path = desk.path("Pages.pdf");
    std::fs::write(
        &path,
        include_bytes!("../fixtures/documents/text-pages.pdf"),
    )
    .unwrap();
    let (mut app, mut h) = desk.launch();
    let state = open(&mut h, &path, "Pages");
    let uid = state["books"]["uid"].clone();
    h.cmd(json!({"cmd":"books","action":"next"}));
    let empty = h.state_until("empty original PDF page", |s| s["books"]["chapter"] == 1);
    assert!(empty["books"]["paragraphs"].as_array().unwrap().is_empty());
    h.cmd(json!({"cmd":"books","action":"next"}));
    h.state_until("third original PDF page", |s| s["books"]["chapter"] == 2);
    mine(&mut h);
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    h.state_until("phrase notebook ready", |s| s["phrases"]["busy"] == false);
    h.cmd(json!({"cmd":"phrases","action":"capture"}));
    let draft = h.state()["phrases"]["editor"].clone();
    assert_eq!(draft["context"], "猫です。");
    assert_eq!(draft["source"]["chapter"], 2);
    assert_eq!(draft["source"]["uid"], uid);
    h.cmd(json!({"cmd":"phrases","action":"edit","text":"猫"}));
    h.cmd(json!({"cmd":"phrases","action":"save"}));
    let saved = h.state_until("PDF phrase saved", |s| {
        s["phrases"]["busy"] == false
            && s["phrases"]["items"]
                .as_array()
                .is_some_and(|p| p.len() == 1)
    });
    let phrase = saved["phrases"]["items"][0].clone();
    h.key("Escape");
    index(&mut h, &uid, &[0, 2]);
    for (name, bytes, expected) in [
        (
            "NoText.pdf",
            include_bytes!("../fixtures/documents/no-text.pdf").as_slice(),
            "no text",
        ),
        (
            "Password.pdf",
            include_bytes!("../fixtures/documents/password.pdf").as_slice(),
            "password",
        ),
        (
            "Invalid.pdf",
            b"%PDF-1.7 invalid".as_slice(),
            "extraction failed",
        ),
    ] {
        let bad = desk.path(name);
        std::fs::write(&bad, bytes).unwrap();
        h.cmd(json!({"cmd":"books","action":"add","path":bad}));
        h.state_until("failed PDF keeps usable reader", |s| {
            s["books"]["busy"] == false
                && s["books"]["status"]
                    .as_str()
                    .is_some_and(|t| t.contains(expected))
        });
        assert_eq!(h.state()["books"]["uid"], uid);
        assert_eq!(h.state()["books"]["chapter"], 2);
    }
    analyse(&mut h, &uid);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":uid}));
    h.state_until("PDF resumes its original page", |s| {
        s["books"]["uid"] == uid && s["books"]["chapter"] == 2
    });
    h.cmd(json!({"cmd":"books","action":"previous"}));
    h.state_until("reader moved away from saved phrase", |s| {
        s["books"]["chapter"] == 1
    });
    h.cmd(json!({"cmd":"phrases","action":"open"}));
    h.state_until("persisted PDF phrase", |s| {
        s["phrases"]["items"]
            .as_array()
            .is_some_and(|p| p.len() == 1)
    });
    h.cmd(json!({"cmd":"phrases","action":"source","id":phrase["id"]}));
    h.state_until("PDF phrase reopens exact source page", |s| {
        s["books"]["uid"] == uid && s["books"]["chapter"] == 2
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}

// T-DOC-004, T-DOC-005: explicit setup reuses an installed helper, saves its path
// and updates both native reader/analysis workers without restarting or elevating.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and installed Poppler"]
fn pdf_helper_setup_reuse_updates_workers_and_survives_restart() {
    let helper = nikomiru_app::document_support::find_pdf("").expect("installed Poppler");
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.set_env("NIKOMIRU_PDFTOTEXT", helper.to_str().unwrap());
    let missing = desk.path("missing helper");
    desk.write_config(&format!(
        "[general]\nui_language=\"en\"\n[subtitles]\nword_colors=true\n[paths]\npdftotext={}\n",
        serde_json::to_string(&missing.to_str().unwrap()).unwrap()
    ));
    let plain = desk.path("Kept.txt");
    let pdf = desk.path("Native.pdf");
    std::fs::write(&plain, "猫です。").unwrap();
    std::fs::write(&pdf, include_bytes!("../fixtures/documents/text-pages.pdf")).unwrap();
    let (mut app, mut h) = desk.launch();
    let kept = open(&mut h, &plain, "Kept")["books"]["uid"].clone();
    h.cmd(json!({"cmd":"books","action":"add","path":pdf}));
    h.state_until("missing configured PDF helper", |s| {
        s["books"]["busy"] == false
            && s["books"]["status"]
                .as_str()
                .is_some_and(|t| t.contains("needs Poppler"))
    });
    assert_eq!(h.state()["books"]["uid"], kept);
    assert_eq!(
        h.state()["setup"]["paths"]["pdftotext"],
        missing.to_str().unwrap()
    );
    h.cmd(json!({"cmd":"setup","action":"open"}));
    h.cmd(json!({"cmd":"setup","action":"clear"}));
    h.cmd(json!({"cmd":"setup","action":"tick","id":"pdftotext"}));
    h.cmd(json!({"cmd":"setup","action":"install"}));
    h.state_until("installed Poppler explicitly adopted and saved", |s| {
        s["setup"]["items"]["pdftotext"] == "Done"
    });
    assert_eq!(
        h.state()["setup"]["paths"]["pdftotext"],
        helper.to_str().unwrap()
    );
    h.cmd(json!({"cmd":"setup","action":"close"}));
    let uid = open(&mut h, &pdf, "Native")["books"]["uid"].clone();
    analyse(&mut h, &uid);
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    desk.set_env("NIKOMIRU_PDFTOTEXT", missing.to_str().unwrap());
    let (mut app, mut h) = desk.launch();
    assert_eq!(
        h.state()["setup"]["paths"]["pdftotext"],
        helper.to_str().unwrap()
    );
    h.cmd(json!({"cmd":"books","action":"open"}));
    h.cmd(json!({"cmd":"books","action":"choose","path":uid}));
    h.state_until("configured helper takes precedence after restart", |s| {
        s["books"]["uid"] == uid
    });
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
