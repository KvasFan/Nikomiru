// SPDX-License-Identifier: GPL-3.0-only

//! Graphic subtitles read by the real model (T-SUB-LIVE-028): pictures of
//! subtitles named by the environment, with their transparency, written as
//! a PGS stream and read as the app reads one. `NIKOMIRU_OCR_DIR`, where the
//! model goes, and `NIKOMIRU_PGS_PICTURES`, `picture=text` pairs separated
//! by `;`, the lines of a text separated by `/`.

use std::path::Path;
use std::time::Instant;

use nikomiru_app::graphic::{read, threads};
use nikomiru_app::ocr::install;
use nikomiru_media::process::Cancel;
use nikomiru_ocr::{Engine, MODEL, grey};
use nikomiru_online::http::{CONNECT_TIMEOUT, Http};
use nikomiru_subs::pgs::{Picture, parse, testing};

/// A picture's pixels as palette indices: 8 levels of grey by 8 of
/// opacity, 0 for the clear ones, as an authoring tool quantizes them.
fn object(picture: &image::RgbaImage) -> Vec<Vec<u8>> {
    picture
        .rows()
        .map(|row| {
            row.map(|p| {
                let [r, g, b, a] = p.0;
                if a < 16 {
                    return 0;
                }
                let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
                1 + (luma >> 5) as u8 * 8 + (a >> 5)
            })
            .collect()
        })
        .collect()
}

fn palette() -> Vec<[u8; 5]> {
    (0..64u8)
        .map(|i| {
            let (luma, alpha) = (u32::from(i / 8) * 32 + 16, u32::from(i % 8) * 32 + 31);
            [i + 1, (16 + luma * 219 / 255) as u8, 128, 128, alpha as u8]
        })
        .collect()
}

// T-SUB-LIVE-028
#[test]
#[ignore = "L7: fetches the real model; run with --profile xtask"]
fn l7_graphic_subtitles_are_read_by_the_real_model() {
    let (Ok(dir), Ok(pictures)) = (
        std::env::var("NIKOMIRU_OCR_DIR"),
        std::env::var("NIKOMIRU_PGS_PICTURES"),
    ) else {
        eprintln!("skipped: no NIKOMIRU_OCR_DIR");
        return;
    };
    let dir = Path::new(&dir);
    let http = Http::with_timeouts("", CONNECT_TIMEOUT, std::time::Duration::from_secs(3600));
    install(&http.unwrap(), &MODEL, dir, &|_| {}).unwrap();
    let engine = Engine::load(dir).unwrap();
    let pairs: Vec<(&str, String)> = pictures
        .split(';')
        .map(|pair| {
            let (path, text) = pair.split_once('=').unwrap();
            (path, text.replace('/', "\n"))
        })
        .collect();
    let mut sup = Vec::new();
    for (n, (path, _)) in pairs.iter().enumerate() {
        let rows = object(&image::open(path).unwrap().to_rgba8());
        let x = (1920 - rows[0].len()) as u16 / 2;
        let y = 1080 - 60 - rows.len() as u16;
        let ms = n as u32 * 3000;
        testing::display_set_with(&mut sup, ms, &[(x, y, rows)], &palette());
        testing::display_set(&mut sup, ms + 2000, &[], 255);
    }
    let cues = parse(&sup);
    assert_eq!(cues.len(), pairs.len());
    let read_line = |line: &Picture| {
        let picture = grey(line.width, line.height, line.grey.clone()).unwrap();
        engine.read(&picture).map_err(|e| e.to_string())
    };
    let started = Instant::now();
    let read = read(&cues, threads(), &Cancel::new(), &read_line)
        .unwrap()
        .unwrap();
    eprintln!(
        "{} pictures in {:?} on {} threads",
        cues.len(),
        started.elapsed(),
        threads()
    );
    let texts: Vec<&str> = read.iter().map(|c| c.2.as_str()).collect();
    for text in &texts {
        eprintln!("{text:?}");
    }
    let wanted: Vec<&str> = pairs.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(texts, wanted);
}
