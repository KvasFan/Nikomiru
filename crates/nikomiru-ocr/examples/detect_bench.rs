// SPDX-License-Identifier: GPL-3.0-only

//! How long finding and reading a whole page takes (MNG-011).
//! `cargo run --profile xtask -p nikomiru-ocr --example detect_bench -- <model dir> <page>…`

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: detect_bench <model dir> <page>…");
        return;
    };
    let dir = std::path::Path::new(&dir);
    let started = Instant::now();
    let (detector, engine) = match (
        nikomiru_ocr::Detector::load(dir),
        nikomiru_ocr::Engine::load(dir),
    ) {
        (Ok(d), Ok(e)) => (d, e),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            return;
        }
    };
    println!("load: {:?}", started.elapsed());
    for path in args {
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("{path}: cannot read");
            continue;
        };
        let started = Instant::now();
        let Ok(page) = nikomiru_ocr::decode(&bytes) else {
            eprintln!("{path}: not a picture");
            continue;
        };
        let blocks = match detector.find(&page) {
            Ok(blocks) => blocks,
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        println!("{path}: {} blocks in {:?}", blocks.len(), started.elapsed());
        let (mut reading, mut laying) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        for block in blocks {
            let [l, t, r, b] = block.bounds;
            let started = Instant::now();
            let text = engine.read(&page.crop_imm(l, t, r - l, b - t));
            reading += started.elapsed();
            let chars = text.as_ref().map_or(0, |t| t.chars().count());
            let started = Instant::now();
            let layout = nikomiru_ocr::layout(&page, &block, chars);
            laying += started.elapsed();
            println!(
                "  {:?} {} {:?}\n    marks {:?}\n    lines {:?}\n    covers {:?}",
                block.bounds,
                if block.vertical { "v" } else { "h" },
                text,
                block.lines,
                layout.lines,
                layout.covers
            );
        }
        println!("{path}: reading {reading:?}, layout {laying:?}");
        println!("{path}: whole page {:?}", started.elapsed());
    }
}
