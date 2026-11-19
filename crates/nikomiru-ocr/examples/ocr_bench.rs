// SPDX-License-Identifier: GPL-3.0-only

//! How long the model takes: loading, then reading each picture given, three
//! times. `cargo run --profile xtask -p nikomiru-ocr --example ocr_bench -- <model dir> <picture>…`

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: ocr_bench <model dir> <picture>…");
        return;
    };
    let started = Instant::now();
    let engine = match nikomiru_ocr::Engine::load(std::path::Path::new(&dir)) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    println!("load: {:?}", started.elapsed());
    let args: Vec<String> = args.collect();
    let pictures: Vec<_> = args
        .iter()
        .filter_map(|path| {
            nikomiru_ocr::crop(&std::fs::read(path).ok()?, [0.0, 0.0, 1.0, 1.0]).ok()
        })
        .collect();
    for _ in 0..2 {
        let started = Instant::now();
        match engine.read_many(&pictures) {
            Ok(texts) => println!("together: {texts:?} ({:?})", started.elapsed()),
            Err(e) => eprintln!("together: {e}"),
        }
    }
    for path in args {
        let Ok(page) = std::fs::read(&path) else {
            eprintln!("{path}: cannot read");
            continue;
        };
        let Ok(picture) = nikomiru_ocr::crop(&page, [0.0, 0.0, 1.0, 1.0]) else {
            eprintln!("{path}: not a picture");
            continue;
        };
        for _ in 0..3 {
            let started = Instant::now();
            match engine.read(&picture) {
                Ok(text) => println!("{path}: {text} ({:?})", started.elapsed()),
                Err(e) => eprintln!("{path}: {e}"),
            }
        }
    }
}
