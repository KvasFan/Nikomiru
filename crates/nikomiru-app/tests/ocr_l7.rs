// SPDX-License-Identifier: GPL-3.0-only

//! The real model (T-MNG-LIVE-001): fetched through the app's own code from
//! where it lives, then run on pictures named by the environment:
//! `NIKOMIRU_OCR_DIR`, where the model goes, and `NIKOMIRU_OCR_PICTURES`,
//! `picture=text` pairs separated by `;`.

use std::path::Path;
use std::time::Instant;

use nikomiru_app::ocr::install;
use nikomiru_ocr::{Engine, MODEL, crop, installed};
use nikomiru_online::http::{CONNECT_TIMEOUT, Http};

#[test]
#[ignore = "L7: fetches the real model; run with --profile xtask"]
fn l7_the_real_model_reads_real_pictures() {
    let (Ok(dir), Ok(pictures)) = (
        std::env::var("NIKOMIRU_OCR_DIR"),
        std::env::var("NIKOMIRU_OCR_PICTURES"),
    ) else {
        eprintln!("skipped: no NIKOMIRU_OCR_DIR");
        return;
    };
    let dir = Path::new(&dir);
    let http = Http::with_timeouts("", CONNECT_TIMEOUT, std::time::Duration::from_secs(3600));
    install(&http.unwrap(), &MODEL, dir, &|_| {}).unwrap();
    assert!(installed(dir));
    let engine = Engine::load(dir).unwrap();
    for pair in pictures.split(';') {
        let (path, wanted) = pair.split_once('=').unwrap();
        let page = std::fs::read(path).unwrap();
        let started = Instant::now();
        let text = engine
            .read(&crop(&page, [0.0, 0.0, 1.0, 1.0]).unwrap())
            .unwrap();
        eprintln!("{path}: {text} ({:?})", started.elapsed());
        assert_eq!(text, wanted);
    }
}
