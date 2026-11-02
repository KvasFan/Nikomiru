// SPDX-License-Identifier: GPL-3.0-only

//! L4: web pages for the Books page through the `online` worker, against the
//! fake online services, with the system's iconv (T-WEB-001).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use nikomiru_app::online::{Online, Parts};
use nikomiru_core::config::{NetworkConfig, OnlineConfig};
use nikomiru_online::fake::FakeOnline;
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::online::{OnlineService, PageSource};

/// 吾輩は猫である in Shift_JIS.
const CAT: [u8; 14] = [
    0x8C, 0xE1, 0x94, 0x79, 0x82, 0xCD, 0x94, 0x4C, 0x82, 0xC5, 0x82, 0xA0, 0x82, 0xE9,
];

fn sjis_page() -> Vec<u8> {
    let mut page =
        b"<html><head><meta http-equiv=\"Content-Type\" content=\"text/html;charset=Shift_JIS\">\
        <title>neko</title></head><body><div class=\"main_text\">"
            .to_vec();
    page.extend(CAT);
    page.extend(b"</div></body></html>");
    page
}

fn online(web: PathBuf) -> Online {
    Online::start(Parts {
        online: OnlineConfig::default(),
        network: NetworkConfig::default(),
        language: "ja".into(),
        subtitles_dir: web.join("subtitles"),
        web_dir: web,
        translator: Translator::from_setting("en", |_| None),
    })
}

fn page(online: &Online, source: PageSource) -> Result<PathBuf, String> {
    let (tx, rx) = mpsc::channel();
    online.page(source, Box::new(move |r| tx.send(r).unwrap()));
    rx.recv_timeout(Duration::from_secs(30)).unwrap()
}

// T-WEB-001: a page at an address and an HTML file, in Shift_JIS, saved in
// UTF-8 with their address and read as a book; a saved page is not saved
// again; only HTTPS goes out
#[test]
fn a_web_page_becomes_a_book() {
    let fake = FakeOnline::start().unwrap();
    fake.data().pages.insert("neko.html".into(), sjis_page());
    let dir = tempfile::tempdir().unwrap();
    let online = online(dir.path().join("web"));
    let address = fake.page_url("neko.html");
    let saved = page(&online, PageSource::Address(address.clone())).unwrap();
    assert!(saved.starts_with(dir.path().join("web")));
    let text = std::fs::read_to_string(&saved).unwrap();
    assert_eq!(
        nikomiru_books::web::address_of(&text),
        Some(address.as_str())
    );
    let book = nikomiru_books::read(&saved).unwrap();
    let host = address
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap();
    assert_eq!((book.title.as_str(), book.author.as_str()), ("neko", host));
    assert_eq!(book.chapters[0].paragraphs[0].text, "吾輩は猫である");
    // the same page again: the same file
    assert_eq!(page(&online, PageSource::Address(address)).unwrap(), saved);
    assert_eq!(
        page(&online, PageSource::File(saved.clone())).unwrap(),
        saved
    );
    // an HTML file of this computer
    let file = dir.path().join("neko.html");
    std::fs::write(&file, sjis_page()).unwrap();
    let copy = page(&online, PageSource::File(file)).unwrap();
    assert_ne!(copy, saved);
    let book = nikomiru_books::read(&copy).unwrap();
    assert_eq!(book.chapters[0].paragraphs[0].text, "吾輩は猫である");
    // PRV-001: plain HTTP only on this computer
    assert!(page(&online, PageSource::Address("http://example.org/a".into())).is_err());
    assert!(page(&online, PageSource::Address(fake.page_url("missing.html"))).is_err());
}
