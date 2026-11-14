// SPDX-License-Identifier: GPL-3.0-only

//! KNW-016: actual book parsing and dictionary segmentation, no GUI writes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_core::{Cancel, ManualClock};
use nikomiru_library::{DictionaryManager, Library};
use nikomiru_ui::{
    book_scores::{Outcome, Request},
    i18n::Translator,
    words::WordStatus,
};
use std::{io::Write, path::Path, sync::Arc, time::SystemTime};

fn manager(root: &Path) -> DictionaryManager {
    let dictionary = root.join("dict.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&dictionary).unwrap());
    for (name, data) in [
        (
            "index.json",
            include_str!("../../../tests/fixtures/dicts/mini-ja/index.json"),
        ),
        (
            "term_bank_1.json",
            include_str!("../../../tests/fixtures/dicts/mini-ja/term_bank_1.json"),
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let library = Arc::new(Library::open(&root.join("library.redb")).unwrap());
    let manager = DictionaryManager::open(
        library,
        root.join("dicts"),
        Arc::new(ManualClock::new(SystemTime::UNIX_EPOCH)),
        "ja",
    )
    .unwrap();
    manager
        .import(&dictionary, None, &Cancel::new(), |_| {})
        .unwrap();
    manager
}

fn request(path: &Path) -> Request {
    let book = nikomiru_books::read(path).unwrap();
    Request {
        uid: nikomiru_library::books::book_uid(&book.title, &book.author, book.chars()),
        path: path.into(),
        generation: 0,
        serial: 1,
    }
}

fn tr() -> Translator {
    Translator::from_setting("en", |_| None)
}

// T-KNW-016: the count includes repeated words across distinct chapters.
#[test]
fn every_chapter_is_counted_with_the_actual_dictionary() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let path = dir.path().join("book.epub");
    nikomiru_books::sample::write(&path).unwrap();
    let outcome =
        nikomiru_app::book_scores::count(&manager, &request(&path), &Cancel::new(), &tr()).unwrap();
    let Outcome::Counted(counted) = outcome else {
        panic!("whole count expected");
    };
    let index = counted
        .words
        .iter()
        .position(|(e, r)| e == "食べる" && r == "たべる")
        .unwrap();
    assert_eq!(counted.occurrences()[index], 2);
    assert_eq!(counted.score(|_, _| WordStatus::Unknown).known, 0);
    assert_eq!(
        counted
            .score(|word, _| if word == "食べる" {
                WordStatus::Known
            } else {
                WordStatus::Unknown
            })
            .known,
        2
    );
}

// T-KNW-016: refresh reads the file again; replaced identity gets no score.
#[test]
fn refresh_reads_new_text_and_rejects_another_book_at_the_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let path = dir.path().join("page.html");
    std::fs::write(&path, "<title>Page</title><main><p>食べる</p></main>").unwrap();
    let request = request(&path);
    std::fs::write(&path, "<title>Page</title><main><p>日本語</p></main>").unwrap();
    let Outcome::Counted(counted) =
        nikomiru_app::book_scores::count(&manager, &request, &Cancel::new(), &tr()).unwrap()
    else {
        panic!("whole count expected");
    };
    assert!(counted.words.iter().any(|(e, _)| e == "日本語"));
    assert!(counted.words.iter().all(|(e, _)| e != "食べる"));
    std::fs::write(
        &path,
        "<title>Another book</title><main><p>日本語</p></main>",
    )
    .unwrap();
    let error =
        nikomiru_app::book_scores::count(&manager, &request, &Cancel::new(), &tr()).unwrap_err();
    assert!(error.contains("book file has changed"), "{error}");
}

// T-KNW-016: excluded, limited, missing and cancelled sources have no count.
#[test]
fn analysis_failures_never_return_a_partial_count() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(dir.path());
    let mut request = Request {
        uid: "book/a".into(),
        path: dir.path().join("manga.cbz"),
        generation: 0,
        serial: 1,
    };
    std::fs::write(&request.path, b"not an archive").unwrap();
    assert!(matches!(
        nikomiru_app::book_scores::count(&manager, &request, &Cancel::new(), &tr()),
        Ok(Outcome::NoText)
    ));
    request.path = dir.path().join("long.html");
    std::fs::write(
        &request.path,
        format!(
            "<p>{}</p>",
            "a".repeat(nikomiru_books::analysis::MAX_PARAGRAPH_BYTES + 1)
        ),
    )
    .unwrap();
    assert!(matches!(
        nikomiru_app::book_scores::count(&manager, &request, &Cancel::new(), &tr()),
        Ok(Outcome::TooLarge)
    ));
    std::fs::remove_file(&request.path).unwrap();
    assert!(nikomiru_app::book_scores::count(&manager, &request, &Cancel::new(), &tr()).is_err());
    let cancel = Cancel::new();
    cancel.cancel();
    assert_eq!(
        nikomiru_app::book_scores::count(&manager, &request, &cancel, &tr()).unwrap_err(),
        "closing"
    );
}

// T-KNW-025: actual dictionary/book counts reuse stable restart keys, invalidate
// dictionary settings/language and changed source, and rescore fresh statuses.
#[test]
fn persistent_book_counts_survive_restart_and_validate_sources_and_dictionaries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("page.html");
    std::fs::write(&path, "<title>Page</title><main><p>食べる</p></main>").unwrap();
    let request = request(&path);
    let cancel = Cancel::new();
    let cache_path = dir.path().join("cache");
    let original_key;
    let original;
    {
        let manager = manager(dir.path());
        let store = nikomiru_app::analysis_cache::Store::new(cache_path.clone());
        original_key =
            nikomiru_app::analysis_cache::key(&manager, &path, &request.uid, &cancel).unwrap();
        let Outcome::Counted(counted) =
            nikomiru_app::book_scores::count_cached(&manager, &request, &cancel, &tr(), &store)
                .unwrap()
        else {
            panic!("complete book expected")
        };
        original = counted;
        assert_eq!(store.get(&original_key, &cancel), Some(original.clone()));
    }
    let library = Arc::new(Library::open(&dir.path().join("library.redb")).unwrap());
    let manager = DictionaryManager::open(
        library,
        dir.path().join("dicts"),
        Arc::new(ManualClock::new(SystemTime::UNIX_EPOCH)),
        "ja",
    )
    .unwrap();
    let store = nikomiru_app::analysis_cache::Store::new(cache_path);
    assert_eq!(
        nikomiru_app::analysis_cache::key(&manager, &path, &request.uid, &cancel),
        Some(original_key.clone())
    );
    let Outcome::Counted(restarted) =
        nikomiru_app::book_scores::count_cached(&manager, &request, &cancel, &tr(), &store)
            .unwrap()
    else {
        panic!("cached book expected")
    };
    assert_eq!(restarted, original);
    assert_eq!(
        restarted.score(|_, _| WordStatus::Known).known,
        restarted.score(|_, _| WordStatus::Unknown).words
    );
    manager.set_study_language("en");
    assert_ne!(
        nikomiru_app::analysis_cache::key(&manager, &path, &request.uid, &cancel),
        Some(original_key.clone())
    );
    manager.set_study_language("ja");
    let id = manager.list().unwrap()[0].0.clone();
    manager.set_enabled(&id, false).unwrap();
    assert_ne!(
        nikomiru_app::analysis_cache::key(&manager, &path, &request.uid, &cancel),
        Some(original_key.clone())
    );
    let Outcome::Counted(disabled) =
        nikomiru_app::book_scores::count_cached(&manager, &request, &cancel, &tr(), &store)
            .unwrap()
    else {
        panic!()
    };
    assert!(disabled.words.is_empty());
    manager.set_enabled(&id, true).unwrap();
    std::fs::write(&path, "<title>Page</title><main><p>日本語</p></main>").unwrap();
    let Outcome::Counted(changed) =
        nikomiru_app::book_scores::count_cached(&manager, &request, &cancel, &tr(), &store)
            .unwrap()
    else {
        panic!()
    };
    assert!(changed.words.iter().any(|(word, _)| word == "日本語"));
    assert!(!changed.words.iter().any(|(word, _)| word == "食べる"));
    cancel.cancel();
    assert!(
        nikomiru_app::book_scores::count_cached(&manager, &request, &cancel, &tr(), &store)
            .is_err()
    );
}
