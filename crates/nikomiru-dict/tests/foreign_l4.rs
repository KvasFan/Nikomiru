// SPDX-License-Identifier: GPL-3.0-only

//! L4: a dictionary of another format, from its file to a lookup (T-DIC-004).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nikomiru_core::Cancel;
use nikomiru_dict::import::{self, DB_FILE, ImportError};
use nikomiru_dict::lookup::{Dictionary, Lookup, Options};
use nikomiru_dict::store::RedbStore;

// T-DIC-004: conjugated forms find it too, since it has no rules at all
#[test]
fn a_dsl_is_looked_up_like_any_dictionary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mini.dsl");
    std::fs::write(
        &path,
        "#NAME \"Mini\"\n\nたべる【食べる】\n\t[b]to eat[/b]\n",
    )
    .unwrap();
    let info = import::read_index(&path).unwrap();
    assert_eq!(info.title, "Mini");
    let dicts = dir.path().join("dicts");
    let imported = import::import(&path, &dicts, &Cancel::new(), |_| {}).unwrap();
    assert_eq!(imported.info.index.title, "Mini");
    let store = RedbStore::open(&imported.dir.join(DB_FILE)).unwrap();
    let lookup = Lookup::new(
        vec![Dictionary {
            id: imported.id.clone(),
            name: "Mini".into(),
            store: Arc::new(store),
        }],
        "ja",
    );
    let entries = lookup.lookup("食べました", Options::default()).unwrap();
    let entry = entries.first().expect("an entry");
    assert_eq!(
        (entry.expression.as_str(), entry.reading.as_str()),
        ("食べる", "たべる")
    );
    assert!(
        entry.glossaries[0]
            .term
            .glossary()
            .unwrap()
            .contains("to eat")
    );
}

// T-DIC-013: a file that cannot be used leaves nothing behind
#[test]
fn a_failed_import_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.mdx");
    std::fs::write(&path, "not a dictionary").unwrap();
    let dicts = dir.path().join("dicts");
    let result = import::import(&path, &dicts, &Cancel::new(), |_| {});
    assert!(matches!(result, Err(ImportError::Foreign { .. })));
    assert_eq!(std::fs::read_dir(&dicts).unwrap().count(), 0);
    let empty = dir.path().join("empty.dsl");
    std::fs::write(&empty, "#NAME \"Empty\"\n").unwrap();
    assert!(matches!(
        import::import(&empty, &dicts, &Cancel::new(), |_| {}),
        Err(ImportError::Empty)
    ));
}

// T-DIC-LIVE-001: a real dictionary, from `NIKOMIRU_REAL_DICT` and, to look up
// in it, `NIKOMIRU_REAL_WORD`
#[test]
#[ignore = "L7: needs a real dictionary"]
fn l7_a_real_dictionary_is_imported_and_looked_up() {
    let Ok(path) = std::env::var("NIKOMIRU_REAL_DICT") else {
        eprintln!("skipped: no NIKOMIRU_REAL_DICT");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = std::path::Path::new(&path);
    eprintln!("{:#?}", import::read_index(path).unwrap());
    let imported = import::import(path, dir.path(), &Cancel::new(), |_| {}).unwrap();
    eprintln!("{:?}", imported.info.counts);
    let store = RedbStore::open(&imported.dir.join(DB_FILE)).unwrap();
    let lookup = Lookup::new(
        vec![Dictionary {
            id: imported.id.clone(),
            name: "Real".into(),
            store: Arc::new(store),
        }],
        "ja",
    );
    let word = std::env::var("NIKOMIRU_REAL_WORD").unwrap_or_else(|_| "食べました".into());
    let entries = lookup.lookup(&word, Options::default()).unwrap();
    for entry in entries.iter().take(4) {
        let text = entry.glossaries[0].term.glossary().unwrap();
        let text: String = text.chars().take(160).collect();
        eprintln!("{} [{}]: {text}", entry.expression, entry.reading);
    }
    assert!(!entries.is_empty(), "nothing for {word}");
}
