// SPDX-License-Identifier: GPL-3.0-only

//! L4: lookups on the imported mini dictionaries with the real `ja.json`
//! deinflection tables (TEST §6, T-LKP-0xx).

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use nikomiru_dict::lookup::{Entry, Lookup, Options, best_length};
use nikomiru_dict::yomitan::{Meta, PitchPosition};

fn setup(names: &[&str]) -> (tempfile::TempDir, Lookup) {
    let tmp = tempfile::tempdir().unwrap();
    let dicts = common::install(tmp.path(), names);
    (tmp, Lookup::new(dicts, "ja"))
}

fn ja() -> (tempfile::TempDir, Lookup) {
    setup(&["mini-ja", "mini-freq", "mini-pitch", "mini-kanji"])
}

fn first(lookup: &Lookup, text: &str) -> Entry {
    let entries = lookup.lookup(text, Options::default()).unwrap();
    entries
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("nothing for {text}"))
}

fn trace(e: &Entry) -> Vec<&str> {
    e.trace.iter().map(|s| s.name.as_str()).collect()
}

// T-LKP-020, T-LKP-021, T-LKP-022, T-LKP-024
#[test]
fn inflected_forms_find_their_lemma() {
    let (_tmp, lookup) = ja();
    for (text, lemma) in [
        ("食べました", "食べる"),
        ("行かなかった", "行く"),
        ("書いた", "書く"),
        ("高くない", "高い"),
        ("読んでいる", "読む"),
    ] {
        let e = first(&lookup, text);
        assert_eq!(
            (e.expression.as_str(), e.matched.as_str()),
            (lemma, text),
            "{text}"
        );
        assert!(!e.trace.is_empty(), "{text}");
    }
    // removal order, outermost first: shown reversed as -て « -いる
    assert_eq!(trace(&first(&lookup, "読んでいる")), ["-いる", "-て"]);
    // a noun is not a verb: the part-of-speech filter drops 物 + -る
    let e = first(&lookup, "食べる");
    assert_eq!((e.expression.as_str(), e.trace.len()), ("食べる", 0));
}

// T-LKP-023
#[test]
fn a_dictionary_without_rules_still_deinflects() {
    let (_tmp, lookup) = setup(&["mini-norules"]);
    let e = first(&lookup, "食べました");
    assert_eq!(
        e.expression, "食べる",
        "empty rules mean any part of speech (BUG-2543)"
    );
}

// T-LKP-030
#[test]
fn the_longest_match_comes_first() {
    let (_tmp, lookup) = ja();
    let entries = lookup.lookup("食べ物を食べる", Options::default()).unwrap();
    assert_eq!(entries[0].expression, "食べ物");
    assert_eq!(best_length(&entries), 3);
    assert!(
        entries
            .iter()
            .any(|e| e.expression == "食" && e.matched == "食")
    );
}

// T-LKP-031
#[test]
fn mixed_kanji_and_kana_spellings() {
    let (_tmp, lookup) = ja();
    let e = first(&lookup, "たべ物を");
    assert_eq!(
        (e.expression.as_str(), e.matched.as_str()),
        ("食べ物", "たべ物")
    );
    // 取扱い is its own spelling in mini-ja
    assert_eq!(first(&lookup, "取扱い").expression, "取扱い");
}

// T-LKP-032
#[test]
fn redirects_are_followed_once_and_real_definitions_kept() {
    let (_tmp, lookup) = ja();
    let entries = lookup.lookup("食べた", Options::default()).unwrap();
    assert!(
        entries.iter().all(|e| e.expression != "食べた"),
        "a pure redirect is not shown"
    );
    assert_eq!(entries[0].expression, "食べる");
    // a label next to a real definition stays (BUG-2566)
    let entries = lookup.lookup("見た", Options::default()).unwrap();
    let mita = entries
        .iter()
        .find(|e| e.expression == "見た")
        .expect("見た kept");
    assert!(
        mita.glossaries[0]
            .term
            .glossary()
            .unwrap()
            .contains("saw (past of")
    );
}

// T-LKP-033
#[test]
fn merging_keeps_the_longest_match_and_spares_tame() {
    let (_tmp, lookup) = ja();
    let e = first(&lookup, "ためになる");
    assert_eq!(
        (e.expression.as_str(), e.matched.as_str()),
        ("ため", "ため")
    );
    // the Kansai -た (言うた) must not swallow the た of ために (BUG-3212)
    let e = first(&lookup, "言うために");
    assert_eq!(
        (e.expression.as_str(), e.matched.as_str()),
        ("言う", "言う")
    );
    assert!(e.trace.is_empty(), "{:?}", trace(&e));
    // but 言うた alone is the Kansai past
    let e = first(&lookup, "言うた");
    assert_eq!(
        (e.expression.as_str(), e.matched.as_str()),
        ("言う", "言うた")
    );
    assert!(trace(&e).contains(&"kansai-ben"), "{:?}", trace(&e));
}

// T-LKP-034
#[test]
fn readings_are_sorted_by_frequency() {
    let (_tmp, lookup) = ja();
    let entries = lookup.lookup("日本", Options::default()).unwrap();
    let readings: Vec<&str> = entries
        .iter()
        .filter(|e| e.expression == "日本")
        .map(|e| e.reading.as_str())
        .collect();
    assert_eq!(readings, ["にほん", "にっぽん"], "300 before 9000");
    let nihon = &entries[0];
    assert_eq!(nihon.frequencies.len(), 1);
    assert_eq!(
        nihon.frequencies[0].values[0].value, 300,
        "only the frequency of its reading"
    );
}

// T-LKP-040, T-LKP-041, T-LKP-043
#[test]
fn pitch_accents_and_ipa_of_the_reading() {
    let (_tmp, lookup) = ja();
    let nihon = first(&lookup, "日本");
    assert_eq!(nihon.pitches.len(), 1);
    let p = &nihon.pitches[0];
    assert_eq!(
        p.pitches.len(),
        1,
        "two equal accents of one dictionary become one"
    );
    assert_eq!(p.pitches[0].position, PitchPosition::Downstep(2));
    assert_eq!(p.ipa[0].ipa, "ɲihoɴ");
    let nippon = lookup
        .lookup("日本", Options::default())
        .unwrap()
        .iter()
        .find(|e| e.reading == "にっぽん")
        .cloned()
        .unwrap();
    assert!(nippon.pitches.is_empty(), "the accents are for にほん");
    let kagami = lookup.lookup("映画", Options::default()).unwrap();
    assert_eq!(
        kagami[0].pitches[0].pitches[0].position,
        PitchPosition::Pattern("LHHH".into())
    );
}

// T-LKP-035
#[test]
fn the_budget_counts_headword_groups() {
    let (_tmp, lookup) = ja();
    let all = lookup.lookup("日本語を", Options::default()).unwrap();
    assert!(all.len() >= 4, "{all:#?}");
    let two = lookup
        .lookup("日本語を", Options { max_results: 2 })
        .unwrap();
    assert_eq!(two.len(), 2);
    assert_eq!(two[0].expression, "日本語");
    assert_eq!(&all[..2], &two[..]);
}

// T-LKP-037
#[test]
fn kanji_entries() {
    let (_tmp, lookup) = ja();
    let found = lookup.kanji("食").unwrap();
    assert_eq!(found.len(), 1);
    let k = &found[0].kanji;
    assert_eq!(
        (k.onyomi.as_str(), k.kunyomi.as_str()),
        ("ショク ジキ", "く.う た.べる")
    );
    assert_eq!(k.meanings, ["eat", "food"]);
    assert!(k.stats.contains(&("strokes".into(), "9".into())));
    assert_eq!(found[0].frequencies[0].values[0].value, 328);
    assert!(lookup.kanji("猫").unwrap().is_empty());
}

// T-LKP-038
#[test]
fn repeated_lookups_come_from_the_cache() {
    let (_tmp, lookup) = ja();
    let a = lookup.lookup("食べました。次", Options::default()).unwrap();
    // only the first 16 characters count
    let b = lookup.lookup("食べました。次", Options::default()).unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    let c = lookup
        .lookup("食べました。次", Options { max_results: 3 })
        .unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&a, &c),
        "another budget is another key"
    );
}

#[test]
fn frequencies_keep_display_values() {
    let (_tmp, lookup) = setup(&["mini-ja", "mini-freq-occ"]);
    let e = first(&lookup, "見る");
    let f = &e.frequencies[0].values[0];
    assert_eq!(
        (f.value, f.display.as_deref()),
        (45_000, Some("45000 (very common)"))
    );
    // occurrence-based: higher is more frequent (DIC-003)
    let entries = lookup.lookup("日本", Options::default()).unwrap();
    assert_eq!(entries[0].reading, "にほん", "30000 occurrences before 800");
    let _ = Meta::mode;
}
