// SPDX-License-Identifier: GPL-3.0-only

//! Graphic subtitles read by the real model (T-SUB-LIVE-028,
//! T-SUB-LIVE-029): pictures of subtitles named by the environment, with
//! their transparency, written as a PGS stream, or made as small and with
//! as few colours as a DVD's and written as VobSub, and read as the app
//! reads them. `NIKOMIRU_OCR_DIR`, where the
//! model goes, and `NIKOMIRU_PGS_PICTURES`, `picture=text` pairs separated
//! by `;`, the lines of a text separated by `/`.

use std::path::Path;
use std::time::Instant;

use nikomiru_app::graphic::{read, threads};
use nikomiru_app::ocr::install;
use nikomiru_media::process::Cancel;
use nikomiru_ocr::{Engine, MODEL, grey};
use nikomiru_online::http::{CONNECT_TIMEOUT, Http};
use nikomiru_subs::pgs::{Graphic, Picture, parse, testing};

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

/// Each picture's file and the text it shows.
type Pairs = Vec<(String, String)>;

/// The model, and the pictures with their texts, from the environment;
/// `None` when it does not name them.
fn inputs() -> Result<Option<(Engine, Pairs)>, String> {
    let (Ok(dir), Ok(pictures)) = (
        std::env::var("NIKOMIRU_OCR_DIR"),
        std::env::var("NIKOMIRU_PGS_PICTURES"),
    ) else {
        eprintln!("skipped: no NIKOMIRU_OCR_DIR");
        return Ok(None);
    };
    let dir = Path::new(&dir);
    let http = Http::with_timeouts("", CONNECT_TIMEOUT, std::time::Duration::from_secs(3600));
    let http = http.map_err(|e| e.to_string())?;
    install(&http, &MODEL, dir, &|_| {}).map_err(|e| e.to_string())?;
    let engine = Engine::load(dir).map_err(|e| e.to_string())?;
    let pairs = pictures
        .split(';')
        .filter_map(|pair| {
            let (path, text) = pair.split_once('=')?;
            Some((path.to_string(), text.replace('/', "\n")))
        })
        .collect();
    Ok(Some((engine, pairs)))
}

/// The cues read by the model, against the texts wanted.
fn read_all<C: Graphic>(engine: &Engine, cues: &[C], pairs: &[(String, String)]) {
    assert_eq!(cues.len(), pairs.len());
    let read_line = |line: &Picture| {
        let picture = grey(line.width, line.height, line.grey.clone()).ok_or("no size")?;
        engine.read(&picture).map_err(|e| e.to_string())
    };
    let started = Instant::now();
    let read = read(cues, threads(), &Cancel::new(), &read_line);
    eprintln!(
        "{} pictures in {:?} on {} threads",
        cues.len(),
        started.elapsed(),
        threads()
    );
    assert!(read.as_ref().is_ok_and(Option::is_some), "{read:?}");
    let read = read.ok().flatten().unwrap_or_default();
    let texts: Vec<&str> = read.iter().map(|c| c.2.as_str()).collect();
    for text in &texts {
        eprintln!("{text:?}");
    }
    let wanted: Vec<&str> = pairs.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(texts, wanted);
}

// T-SUB-LIVE-028
#[test]
#[ignore = "L7: fetches the real model; run with --profile xtask"]
fn l7_graphic_subtitles_are_read_by_the_real_model() {
    let Some((engine, pairs)) = inputs().unwrap() else {
        return;
    };
    let mut sup = Vec::new();
    for (n, (path, _)) in pairs.iter().enumerate() {
        let rows = object(&image::open(path).unwrap().to_rgba8());
        let x = (1920 - rows[0].len()) as u16 / 2;
        let y = 1080 - 60 - rows.len() as u16;
        let ms = n as u32 * 3000;
        testing::display_set_with(&mut sup, ms, &[(x, y, rows)], &palette());
        testing::display_set(&mut sup, ms + 2000, &[], 255);
    }
    read_all(&engine, &parse(&sup), &pairs);
}

/// A picture of a 1080-line screen as a DVD has it: 480 lines, and four
/// slots: clear, the text, its edge and the half-clear shade between.
fn dvd_slots(picture: &image::RgbaImage) -> Vec<Vec<u8>> {
    let (w, h) = (picture.width() * 4 / 9, picture.height() * 4 / 9);
    let small = image::imageops::resize(picture, w, h, image::imageops::FilterType::Triangle);
    small
        .rows()
        .map(|row| {
            row.map(|p| {
                let [r, g, b, a] = p.0;
                let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
                match (a, luma) {
                    (0..64, _) => 0,
                    (64..160, _) => 3,
                    (_, 128..) => 1,
                    _ => 2,
                }
            })
            .collect()
        })
        .collect()
}

// T-SUB-LIVE-029
#[test]
#[ignore = "L7: fetches the real model; run with --profile xtask"]
fn l7_dvd_subtitles_are_read_by_the_real_model() {
    use nikomiru_subs::vobsub::{self, testing as dvd};
    let Some((engine, pairs)) = inputs().unwrap() else {
        return;
    };
    let mut spus = Vec::new();
    for (n, (path, _)) in pairs.iter().enumerate() {
        let rows = dvd_slots(&image::open(path).unwrap().to_rgba8());
        let x = (720 - rows[0].len().min(720)) as u16 / 2;
        let y = 480 - 30 - rows.len() as u16;
        let ms = n as u32 * 3000;
        spus.push((ms, dvd::spu_with(x, y, &rows, 2000, [0, 15, 15, 8])));
    }
    let (ps, _) = dvd::program_stream(&spus, 2000);
    read_all(&engine, &vobsub::parse(&ps, Some(dvd::PALETTE)), &pairs);
}
