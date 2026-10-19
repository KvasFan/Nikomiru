// SPDX-License-Identifier: GPL-3.0-only

//! Helpers shared by the integration tests: the mini dictionaries of
//! `tests/fixtures/dicts/`, zipped and imported.

#![allow(dead_code, clippy::unwrap_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nikomiru_core::Cancel;
use nikomiru_dict::import::import;
use nikomiru_dict::lookup::Dictionary;
use nikomiru_dict::store::{DictStore, RedbStore};

pub fn source(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/dicts")
        .join(name)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            collect(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Zips a fixture folder into `dir`.
pub fn zipped(dir: &Path, name: &str, extra: &[(&str, Vec<u8>)]) -> PathBuf {
    let src = source(name);
    let mut files = Vec::new();
    collect(&src, &mut files);
    files.sort();
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for f in files {
        let inner = f
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(inner, options).unwrap();
        zip.write_all(&std::fs::read(&f).unwrap()).unwrap();
    }
    // extra entries are stored: a compressible payload would trip the bomb check
    let stored = options.compression_method(zip::CompressionMethod::Stored);
    for (inner, bytes) in extra {
        zip.start_file(*inner, stored).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// Imports the named fixture folders, in order, into `dir/dicts`.
pub fn install(dir: &Path, names: &[&str]) -> Vec<Dictionary> {
    names
        .iter()
        .map(|name| {
            let zip = zipped(dir, name, &[]);
            let imported = import(&zip, &dir.join("dicts"), &Cancel::new(), |_| {}).unwrap();
            let store = RedbStore::open(&imported.dir.join("dict.redb")).unwrap();
            Dictionary {
                id: imported.id,
                name: store.info().index.title.clone(),
                store: Arc::new(store),
            }
        })
        .collect()
}
