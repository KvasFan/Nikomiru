// SPDX-License-Identifier: GPL-3.0-only

//! L1: merging, sorting, budget and grouping of lookups (LKP-033…LKP-036) on
//! dictionaries held in memory.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use nikomiru_dict::lookup::{Dictionary, Entry, Lookup, Options};
use nikomiru_dict::memory::MemStore;
use nikomiru_dict::yomitan::{Frequency, FrequencyMode, Meta, Term};

fn term(expression: &str, reading: &str, rules: &str, score: f64, gloss: &str) -> Term {
    Term {
        expression: expression.into(),
        reading: reading.into(),
        definition_tags: String::new(),
        rules: rules.into(),
        score,
        glossary: format!("[{gloss:?}]"),
        sequence: 0,
        term_tags: String::new(),
    }
}

fn freq(text: &str, reading: Option<&str>, value: i64) -> (String, Meta) {
    (
        text.into(),
        Meta::Frequency(Frequency {
            reading: reading.map(Into::into),
            value,
            display: None,
        }),
    )
}

fn dict(store: MemStore) -> Dictionary {
    let name = store_name(&store);
    Dictionary {
        id: name.clone(),
        name,
        store: Arc::new(store),
    }
}

fn store_name(store: &MemStore) -> String {
    use nikomiru_dict::store::DictStore;
    store.info().index.title.clone()
}

fn lookup(dicts: Vec<MemStore>, language: &str) -> Lookup {
    Lookup::new(dicts.into_iter().map(dict).collect(), language)
}

fn heads(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| format!("{}/{}", e.expression, e.reading))
        .collect()
}

fn order(l: &Lookup, text: &str) -> Vec<String> {
    heads(&l.lookup(text, Options::default()).unwrap())
}

// T-LKP-034 (1) the longest match
#[test]
fn sort_longest_match() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("猫", "ねこ", "", 100.0, "cat"),
            term("猫舌", "ねこじた", "", 0.0, "x"),
        ])],
        "ja",
    );
    assert_eq!(order(&l, "猫舌"), ["猫舌/ねこじた", "猫/ねこ"]);
}

// T-LKP-034 (2) fewer preprocessor steps
#[test]
fn sort_preprocessor_steps() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("かたかな", "かたかな", "", 100.0, "hiragana spelling"),
            term("カタカナ", "カタカナ", "", 0.0, "katakana"),
        ])],
        "ja",
    );
    assert_eq!(
        order(&l, "カタカナ"),
        ["カタカナ/カタカナ", "かたかな/かたかな"]
    );
}

// T-LKP-034 (3) the shorter deinflection trace
#[test]
fn sort_trace_length() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("walk", "walk", "v", 100.0, "to walk"),
            term("walked", "walked", "adj", 0.0, "as an adjective"),
        ])],
        "en",
    );
    assert_eq!(order(&l, "walked"), ["walked/walked", "walk/walk"]);
}

// T-LKP-034 (4) a headword equal to the form looked up
#[test]
fn sort_headword_equals_form() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("為", "ため", "", 100.0, "sake"),
            term("ため", "ため", "", 0.0, "benefit"),
        ])],
        "ja",
    );
    assert_eq!(order(&l, "ため"), ["ため/ため", "為/ため"]);
}

// T-LKP-034 (5) frequency, dictionary by dictionary in the user's order
#[test]
fn sort_frequency() {
    let terms = || {
        vec![
            term("日本", "にほん", "", 0.0, "Japan"),
            term("日本", "にっぽん", "", 100.0, "Japan"),
        ]
    };
    let rank = MemStore::new("rank").with_meta(vec![
        freq("日本", Some("にほん"), 300),
        freq("日本", Some("にっぽん"), 9000),
    ]);
    let l = lookup(vec![MemStore::new("A").with_terms(terms()), rank], "ja");
    assert_eq!(
        order(&l, "日本"),
        ["日本/にほん", "日本/にっぽん"],
        "lower rank first, before score"
    );

    // occurrence counts: higher first (DIC-003); the first dictionary decides
    let occ = MemStore::new("occ")
        .with_index(|i| i.frequency_mode = FrequencyMode::OccurrenceBased)
        .with_meta(vec![
            freq("日本", Some("にほん"), 10),
            freq("日本", Some("にっぽん"), 500),
        ]);
    let rank = MemStore::new("rank").with_meta(vec![
        freq("日本", Some("にほん"), 300),
        freq("日本", Some("にっぽん"), 9000),
    ]);
    let l = lookup(
        vec![MemStore::new("A").with_terms(terms()), occ, rank],
        "ja",
    );
    assert_eq!(order(&l, "日本"), ["日本/にっぽん", "日本/にほん"]);

    // a missing value goes last
    let partial = MemStore::new("partial").with_meta(vec![freq("日本", Some("にっぽん"), 9000)]);
    let l = lookup(vec![MemStore::new("A").with_terms(terms()), partial], "ja");
    assert_eq!(order(&l, "日本"), ["日本/にっぽん", "日本/にほん"]);
}

// T-LKP-034 (6) dictionary priority, only when 1–5 are tied, before the score
#[test]
fn sort_dictionary_priority() {
    let a = MemStore::new("A").with_terms(vec![term("日本", "にっぽん", "", 0.0, "Japan (A)")]);
    let b = MemStore::new("B").with_terms(vec![term("日本", "にほん", "", 100.0, "Japan (B)")]);
    let l = lookup(vec![a, b], "ja");
    assert_eq!(order(&l, "日本"), ["日本/にっぽん", "日本/にほん"]);
    // a frequency still wins over priority
    let a = MemStore::new("A").with_terms(vec![term("日本", "にっぽん", "", 0.0, "Japan (A)")]);
    let b = MemStore::new("B").with_terms(vec![term("日本", "にほん", "", 100.0, "Japan (B)")]);
    let f = MemStore::new("F").with_meta(vec![freq("日本", Some("にほん"), 1)]);
    let l = lookup(vec![a, b, f], "ja");
    assert_eq!(order(&l, "日本"), ["日本/にほん", "日本/にっぽん"]);
}

// T-LKP-034 (7) the higher score
#[test]
fn sort_score() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("日本", "にほん", "", 1.0, "Japan"),
            term("日本", "にっぽん", "", 10.0, "Japan"),
        ])],
        "ja",
    );
    assert_eq!(order(&l, "日本"), ["日本/にっぽん", "日本/にほん"]);
}

// T-LKP-034 (8) a headword that is its own reading
#[test]
fn sort_kana_headword() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("ため", "タメ", "", 0.0, "x"),
            term("ため", "ため", "", 0.0, "y"),
        ])],
        "ja",
    );
    assert_eq!(order(&l, "ため"), ["ため/ため", "ため/タメ"]);
}

// T-LKP-035
#[test]
fn the_budget_keeps_whole_groups() {
    let mut terms = Vec::new();
    for (expression, reading, n, score) in [
        ("日本", "にほん", 4, 20.0),
        ("日本", "にっぽん", 3, 10.0),
        ("日", "ひ", 3, 30.0),
    ] {
        for i in 0..n {
            terms.push(term(
                expression,
                reading,
                "",
                score,
                &format!("{expression} {i}"),
            ));
        }
    }
    let l = lookup(vec![MemStore::new("A").with_terms(terms)], "ja");
    let two = l.lookup("日本", Options { max_results: 2 }).unwrap();
    assert_eq!(heads(&two), ["日本/にほん", "日本/にっぽん"]);
    assert_eq!(
        two[0].glossaries.len(),
        4,
        "a group is never cut (BUG-1472)"
    );
    assert_eq!(two[1].glossaries.len(), 3);
}

// T-LKP-036
#[test]
fn groups_merge_dictionaries_in_order() {
    let a = MemStore::new("A")
        .with_terms(vec![
            term("猫", "ねこ", "n", 1.0, "cat (A)"),
            term("ねこ", "", "n", 1.0, "cat, kana (A)"),
        ])
        .with_meta(vec![
            freq("猫", None, 5),
            freq("猫", None, 5),
            freq("猫", None, 7),
        ]);
    let b = MemStore::new("B").with_terms(vec![
        term("猫", "ねこ", "n", 3.0, "cat (B)"),
        term("ねこ", "ねこ", "", 1.0, "kana (B)"),
    ]);
    let l = lookup(vec![a, b], "ja");
    let entries = l.lookup("猫", Options::default()).unwrap();
    assert_eq!(heads(&entries), ["猫/ねこ"]);
    let neko = &entries[0];
    let glosses: Vec<String> = neko
        .glossaries
        .iter()
        .map(|g| g.term.glossary().unwrap())
        .collect();
    assert_eq!(glosses, [r#"["cat (A)"]"#, r#"["cat (B)"]"#]);
    assert_eq!(
        neko.glossaries
            .iter()
            .map(|g| g.dictionary)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(neko.score, 3.0);
    let values: Vec<i64> = neko.frequencies[0].values.iter().map(|f| f.value).collect();
    assert_eq!(
        values,
        [5, 7],
        "repeated values of a dictionary appear once"
    );
    // an empty reading is the headword itself: one group
    let entries = l.lookup("ねこ", Options::default()).unwrap();
    let kana = entries.iter().find(|e| e.expression == "ねこ").unwrap();
    assert_eq!(kana.glossaries.len(), 2, "{entries:#?}");
}

// T-LKP-023
#[test]
fn empty_rules_are_not_inflectable_where_other_entries_have_rules() {
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![
            term("高い", "たかい", "", 0.0, "high"),
            term("食べる", "たべる", "v1", 0.0, "eat"),
        ])],
        "ja",
    );
    assert!(
        l.lookup("高くない", Options::default())
            .unwrap()
            .iter()
            .all(|e| e.expression != "高い")
    );
    let l = lookup(
        vec![MemStore::new("A").with_terms(vec![term("高い", "たかい", "", 0.0, "high")])],
        "ja",
    );
    assert_eq!(
        order(&l, "高くない")[0],
        "高い/たかい",
        "no rules anywhere: empty means any (BUG-2543)"
    );
}

// LKP-012
#[test]
fn languages_come_from_the_study_language_and_the_dictionaries() {
    let en = MemStore::new("EN")
        .with_index(|i| i.source_language = Some("en".into()))
        .with_terms(vec![term("walk", "walk", "v", 0.0, "to walk")]);
    let l = lookup(vec![en], "ja");
    assert_eq!(
        order(&l, "walked"),
        ["walk/walk"],
        "English rules from the dictionary's language"
    );
    let plain = MemStore::new("plain").with_terms(vec![term("walk", "walk", "v", 0.0, "to walk")]);
    let l = lookup(vec![plain], "ja");
    assert!(
        order(&l, "walked").is_empty(),
        "Japanese only: no English rules"
    );
}
