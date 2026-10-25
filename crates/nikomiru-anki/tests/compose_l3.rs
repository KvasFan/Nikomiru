// SPDX-License-Identifier: GPL-3.0-only

//! L3: the card fields of real lookups on the mini dictionaries (T-ANK-021…
//! T-ANK-025), as HTML snapshots, and the hostile entry of `mini-ja` reduced to
//! the allowlist (T-SEC-001 for cards).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nikomiru_anki::compose::{self, DictionaryFields, Source};
use nikomiru_core::Cancel;
use nikomiru_dict::css::Sheet;
use nikomiru_dict::import::import;
use nikomiru_dict::lookup::{Dictionary, Lookup, Options};
use nikomiru_dict::store::{DictStore, RedbStore};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/dicts")
        .join(name)
}

fn zipped(dir: &Path, name: &str) -> PathBuf {
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
    let src = fixture(name);
    let mut files = Vec::new();
    collect(&src, &mut files);
    files.sort();
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for f in files {
        let inner = f
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(inner, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&std::fs::read(&f).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    path
}

struct Dicts {
    _tmp: tempfile::TempDir,
    lookup: Lookup,
    sheets: Vec<Sheet>,
}

fn dictionaries() -> Dicts {
    let tmp = tempfile::tempdir().unwrap();
    let dicts: Vec<Dictionary> = ["mini-ja", "mini-freq", "mini-pitch"]
        .iter()
        .map(|name| {
            let zip = zipped(tmp.path(), name);
            let imported = import(&zip, &tmp.path().join("dicts"), &Cancel::new(), |_| {}).unwrap();
            let store = RedbStore::open(&imported.dir.join("dict.redb")).unwrap();
            Dictionary {
                id: imported.id,
                name: format!("{} (renamed)", store.info().index.title),
                store: Arc::new(store),
            }
        })
        .collect();
    let sheets = dicts
        .iter()
        .map(|d| Sheet::parse(&d.store.info().styles))
        .collect();
    Dicts {
        _tmp: tmp,
        lookup: Lookup::new(dicts, "ja"),
        sheets,
    }
}

fn fields(d: &Dicts, text: &str) -> DictionaryFields {
    let entries = d.lookup.lookup(text, Options::default()).unwrap();
    let entry = entries
        .first()
        .unwrap_or_else(|| panic!("nothing for {text}"));
    let sources: Vec<Source<'_>> = d
        .lookup
        .dictionaries()
        .iter()
        .zip(&d.sheets)
        .map(|(dict, sheet)| Source {
            title: &dict.store.info().index.title,
            name: &dict.name,
            sheet,
        })
        .collect();
    let dicts = d.lookup.dictionaries();
    compose::dictionary_fields(entry, &sources, &mut |i, path| {
        dicts
            .get(i)
            .and_then(|dict| dict.store.media(path).ok().flatten())
    })
}

fn shown(f: &DictionaryFields) -> String {
    let mut out = String::new();
    for (name, value) in [
        ("expression", &f.expression),
        ("reading", &f.reading),
        ("furigana-plain", &f.furigana_plain),
        ("glossary", &f.glossary),
        ("frequencies", &f.frequencies),
        ("frequency-harmonic-rank", &f.frequency_rank),
        ("pitch-accent-positions", &f.pitch_positions),
        ("pitch-accent-categories", &f.pitch_categories),
        ("phonetic-transcriptions", &f.phonetic),
        ("tags", &f.tags),
    ] {
        out.push_str(&format!("{name}: {value}\n"));
    }
    for (title, name, html) in &f.glossaries {
        out.push_str(&format!("single-glossary-{title} ({name}): {html}\n"));
    }
    for (name, bytes) in &f.media {
        out.push_str(&format!("media {name}: {} bytes\n", bytes.len()));
    }
    out
}

// T-ANK-021, T-ANK-023, T-ANK-025
#[test]
fn a_verb_with_frequency_and_pitch() {
    let d = dictionaries();
    insta::assert_snapshot!(shown(&fields(&d, "食べました")));
}

// T-ANK-021, T-ANK-025
#[test]
fn two_readings_two_accents_and_ipa() {
    let d = dictionaries();
    insta::assert_snapshot!(shown(&fields(&d, "日本")));
}

// T-ANK-023: ruby, an image uploaded with the card, a link written as text
#[test]
fn structured_content_with_an_image() {
    let d = dictionaries();
    let f = fields(&d, "漢字");
    assert_eq!(f.media.len(), 1, "the image goes with the card");
    assert!(f.media[0].0.starts_with("nikomiru_") && f.media[0].0.ends_with(".png"));
    insta::assert_snapshot!(shown(&f));
}

// T-ANK-023: a table and the dictionary's CSS, scoped
#[test]
fn a_table_and_the_dictionary_css() {
    let d = dictionaries();
    insta::assert_snapshot!(shown(&fields(&d, "表")));
}

// T-SEC-001 for cards
#[test]
fn the_hostile_entry_keeps_only_the_allowlist() {
    let d = dictionaries();
    let f = fields(&d, "危険");
    for bad in [
        "<script",
        "alert",
        "url(",
        "evil.example",
        "javascript:",
        " onclick=",
    ] {
        assert!(!f.glossary.contains(bad), "{bad}: {}", f.glossary);
    }
    assert!(f.media.is_empty(), "no remote image");
    insta::assert_snapshot!(shown(&f));
}
