// SPDX-License-Identifier: GPL-3.0-only

//! From a query to the headwords shown in the popup (LKP-030…LKP-038): a port
//! of Fushi's `native/fushidicts/fushidicts_src/lookup.cpp` and `query.cpp`
//! (commit ac8034e), on [`DictStore`].
//!
//! Every prefix of the query (LKP-004) goes through the preprocessors
//! (LKP-010…012) and the deinflector (LKP-020…024); each form is looked up in
//! the enabled term dictionaries, by headword and by reading. Hits merge by
//! headword and reading, keeping the longest match (LKP-033); frequencies are
//! added, the groups sorted (LKP-034) and cut to the budget (LKP-035), and
//! only then pitch accents are read.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use nikomiru_text::scan::{SCAN_LENGTH, candidates};
use nikomiru_text::variants::{Group, groups_for_all, variants};

use crate::deinflect::Deinflector;
use crate::mixed;
use crate::store::{DictReader, DictStore, Field, StoreError, StoredTerm};
use crate::yomitan::{Frequency, FrequencyMode, Ipa, Kanji, Meta, Pitch, parse_redirects};

/// Results kept by the cache (LKP-038).
pub const CACHE_SIZE: usize = 5000;

/// The transform whose matches must not swallow `ため`/`たび` (BUG-3212).
const KANSAI: &str = "kansai-ben";
const KANSAI_PROTECTED: [&str; 6] = [
    "ために",
    "ためには",
    "ためにも",
    "ための",
    "たびに",
    "たびには",
];

/// An enabled dictionary, in the user's order (DIC-031).
#[derive(Clone)]
pub struct Dictionary {
    /// Its id in the registry (its folder name).
    pub id: String,
    /// The name shown.
    pub name: String,
    /// Its entries.
    pub store: Arc<dyn DictStore>,
}

impl std::fmt::Debug for Dictionary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dictionary")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

/// Lookup settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Headword groups returned (LKP-035).
    pub max_results: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { max_results: 10 }
    }
}

/// One step of a trace (LKP-024): a transform or a redirect's rule. Shared
/// strings: a lookup copies traces hundreds of times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Name (`-て`).
    pub name: Arc<str>,
    /// What it means (empty for redirects).
    pub description: Arc<str>,
}

/// A definition of a headword and the dictionary it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Glossary {
    /// Index into [`Lookup::dictionaries`].
    pub dictionary: usize,
    /// The record, glossary still compressed.
    pub term: StoredTerm,
}

/// The frequencies one dictionary gives a headword (LKP-040).
#[derive(Debug, Clone, PartialEq)]
pub struct Frequencies {
    /// Index into [`Lookup::dictionaries`].
    pub dictionary: usize,
    /// Values, as the dictionary lists them, without duplicates.
    pub values: Vec<Frequency>,
}

/// The pitch accents and IPA one dictionary gives a headword (LKP-041).
#[derive(Debug, Clone, PartialEq)]
pub struct Pitches {
    /// Index into [`Lookup::dictionaries`].
    pub dictionary: usize,
    /// Accents, without duplicates.
    pub pitches: Vec<Pitch>,
    /// Transcriptions, without duplicates.
    pub ipa: Vec<Ipa>,
}

/// A headword group (LKP-036).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Headword.
    pub expression: String,
    /// Reading (the headword itself when the dictionary leaves it empty).
    pub reading: String,
    /// The prefix of the query that found it.
    pub matched: String,
    /// The form looked up.
    pub deinflected: String,
    /// Transforms removed, outermost first.
    pub trace: Vec<Step>,
    /// Preprocessors that changed the text.
    pub preprocessor_steps: u32,
    /// Part-of-speech rules of all its records.
    pub rules: String,
    /// The highest score of its records.
    pub score: f64,
    /// Definitions in dictionary order.
    pub glossaries: Vec<Glossary>,
    /// Frequencies in dictionary order.
    pub frequencies: Vec<Frequencies>,
    /// Pitch accents in dictionary order.
    pub pitches: Vec<Pitches>,
}

impl Entry {
    fn reading_or_expression(&self) -> &str {
        if self.reading.is_empty() {
            &self.expression
        } else {
            &self.reading
        }
    }
}

/// The longest match among `entries`, in characters: the text to highlight
/// (LKP-036).
pub fn best_length(entries: &[Entry]) -> usize {
    entries
        .iter()
        .map(|e| e.matched.chars().count())
        .max()
        .unwrap_or(0)
}

/// A kanji entry (LKP-037).
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiEntry {
    /// Index into [`Lookup::dictionaries`].
    pub dictionary: usize,
    /// The entry.
    pub kanji: Kanji,
    /// Frequencies from the dictionaries with kanji meta.
    pub frequencies: Vec<Frequencies>,
}

/// Records of one form, merged across dictionaries by headword and reading.
#[derive(Debug, Clone)]
struct Raw {
    expression: String,
    reading: String,
    rules: String,
    score: f64,
    hits: Vec<Hit>,
}

/// A record found while scanning; its glossary is read only if its group is
/// shown.
#[derive(Debug, Clone, Copy)]
struct Hit {
    dictionary: usize,
    id: u64,
    redirect: bool,
}

/// A group while scanning: the entry without glossaries, and its records.
#[derive(Debug, Clone)]
struct Candidate {
    entry: Entry,
    hits: Vec<Hit>,
}

type Key = (String, String);

fn key_of(expression: &str, reading: &str) -> Key {
    let reading = if reading.is_empty() {
        expression
    } else {
        reading
    };
    (expression.to_string(), reading.to_string())
}

/// A small least-recently-used map.
#[derive(Debug)]
struct Lru<V> {
    capacity: usize,
    tick: u64,
    map: HashMap<String, (V, u64)>,
    order: BTreeMap<u64, String>,
}

impl<V: Clone> Lru<V> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            tick: 0,
            map: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<V> {
        self.tick += 1;
        let (value, used) = self.map.get_mut(key)?;
        self.order.remove(used);
        *used = self.tick;
        self.order.insert(self.tick, key.to_string());
        Some(value.clone())
    }

    fn put(&mut self, key: String, value: V) {
        self.tick += 1;
        if let Some((_, used)) = self.map.insert(key.clone(), (value, self.tick)) {
            self.order.remove(&used);
        }
        self.order.insert(self.tick, key);
        while self.map.len() > self.capacity {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            self.map.remove(&oldest);
        }
    }
}

/// Lookups over a fixed set of enabled dictionaries. Build a new one when the
/// dictionaries change: that also empties the cache (LKP-038).
pub struct Lookup {
    dictionaries: Vec<Dictionary>,
    deinflectors: Vec<Deinflector>,
    /// The transforms of each deinflector as trace steps.
    steps: Vec<Vec<Step>>,
    groups: Vec<Group>,
    cache: Mutex<Lru<Arc<Vec<Entry>>>>,
}

impl std::fmt::Debug for Lookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lookup")
            .field("dictionaries", &self.dictionaries)
            .field("groups", &self.groups)
            .finish_non_exhaustive()
    }
}

/// One form to look up, and where it came from.
struct Form<'a> {
    text: &'a str,
    conditions: u64,
    deinflector: usize,
    trace: Vec<Step>,
}

/// State of one lookup.
struct Run<'a> {
    lookup: &'a Lookup,
    /// One reader per dictionary, for the whole lookup.
    readers: Vec<Box<dyn DictReader + 'a>>,
    window: &'a str,
    results: BTreeMap<Key, Candidate>,
    raw: HashMap<String, Arc<Vec<Raw>>>,
    run_readings: HashMap<String, Vec<String>>,
}

impl Lookup {
    /// A lookup over `dictionaries` (enabled ones only, in the user's order).
    /// The preprocessors and the deinflection tables are those of the study
    /// language and of the languages the dictionaries declare (LKP-012);
    /// Japanese if none is known.
    pub fn new(dictionaries: Vec<Dictionary>, study_language: &str) -> Self {
        let mut languages: Vec<String> = Vec::new();
        let declared = dictionaries
            .iter()
            .filter_map(|d| d.store.info().index.source_language.clone());
        for lang in std::iter::once(study_language.to_string()).chain(declared) {
            if !lang.is_empty() && !languages.contains(&lang) {
                languages.push(lang);
            }
        }
        if languages.is_empty() {
            languages.push("ja".into());
        }
        let deinflectors: Vec<Deinflector> = languages
            .iter()
            .filter_map(|l| Deinflector::for_language(l))
            .collect();
        let steps = deinflectors
            .iter()
            .map(|d| {
                d.transforms()
                    .iter()
                    .map(|t| Step {
                        name: t.name.as_str().into(),
                        description: t.description.as_str().into(),
                    })
                    .collect()
            })
            .collect();
        Self {
            groups: groups_for_all(languages.iter().map(String::as_str)),
            deinflectors,
            steps,
            dictionaries,
            cache: Mutex::new(Lru::new(CACHE_SIZE)),
        }
    }

    /// The dictionaries, in order.
    pub fn dictionaries(&self) -> &[Dictionary] {
        &self.dictionaries
    }

    /// The headwords `text` starts with, best first (LKP-030…LKP-036). Only the
    /// first [`SCAN_LENGTH`] characters count.
    pub fn lookup(&self, text: &str, options: Options) -> Result<Arc<Vec<Entry>>, StoreError> {
        let window_end = text
            .char_indices()
            .nth(SCAN_LENGTH)
            .map_or(text.len(), |(i, _)| i);
        let window = &text[..window_end];
        let key = format!("{}\n{window}", options.max_results);
        if let Some(hit) = self.cache.lock().ok().and_then(|mut c| c.get(&key)) {
            return Ok(hit);
        }
        let entries = Arc::new(self.run(window, options)?);
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(key, Arc::clone(&entries));
        }
        Ok(entries)
    }

    /// The headword a lookup of `text` shows first, without the cache: the
    /// segmentation's lookups (LKP-060) would push the popup's out of it.
    pub(crate) fn first(&self, text: &str) -> Result<Option<Entry>, StoreError> {
        let window_end = text
            .char_indices()
            .nth(SCAN_LENGTH)
            .map_or(text.len(), |(i, _)| i);
        let entries = self.run(&text[..window_end], Options { max_results: 1 })?;
        Ok(entries.into_iter().next())
    }

    fn run(&self, window: &str, options: Options) -> Result<Vec<Entry>, StoreError> {
        let readers = self
            .dictionaries
            .iter()
            .map(|d| d.store.reader())
            .collect::<Result<Vec<_>, _>>()?;
        let mut run = Run {
            lookup: self,
            readers,
            window,
            results: BTreeMap::new(),
            raw: HashMap::new(),
            run_readings: HashMap::new(),
        };
        run.scan()?;
        run.finish(options)
    }

    /// The kanji entries of one character (LKP-037), in dictionary order.
    pub fn kanji(&self, character: &str) -> Result<Vec<KanjiEntry>, StoreError> {
        let mut out = Vec::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            if !d.store.info().kinds.kanji {
                continue;
            }
            if let Some(kanji) = d.store.kanji(character)? {
                out.push(KanjiEntry {
                    dictionary: i,
                    kanji,
                    frequencies: Vec::new(),
                });
            }
        }
        if out.is_empty() {
            return Ok(out);
        }
        let mut frequencies = Vec::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            if d.store.info().counts.kanji_meta == 0 {
                continue;
            }
            let values = dedup(d.store.kanji_meta(character)?.into_iter().filter_map(
                |m| match m {
                    Meta::Frequency(f) => Some(f),
                    _ => None,
                },
            ));
            if !values.is_empty() {
                frequencies.push(Frequencies {
                    dictionary: i,
                    values,
                });
            }
        }
        for e in &mut out {
            e.frequencies.clone_from(&frequencies);
        }
        Ok(out)
    }

    /// Records of `text` in the term dictionaries: equal headword or reading,
    /// merged by headword and reading, glossaries in dictionary order.
    fn query(
        &self,
        readers: &[Box<dyn DictReader + '_>],
        text: &str,
    ) -> Result<Vec<Raw>, StoreError> {
        let mut merged: BTreeMap<Key, Raw> = BTreeMap::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            let info = d.store.info();
            if !info.kinds.terms {
                continue;
            }
            let mut terms = readers[i].heads(Field::Expression, text)?;
            terms.extend(readers[i].heads(Field::Reading, text)?);
            terms.sort_by_key(|t| t.id);
            terms.dedup_by_key(|t| t.id);
            // a dictionary without rules anywhere: empty means any (BUG-2543)
            let wildcard = info.counts.with_rules == 0;
            for term in terms {
                let rules = if wildcard && term.rules.trim().is_empty() {
                    "*"
                } else {
                    term.rules.as_str()
                };
                let key = key_of(&term.expression, &term.reading);
                let raw = merged
                    .entry(key)
                    .or_insert_with_key(|(expression, reading)| Raw {
                        // an empty reading is the headword itself (LKP-036)
                        expression: expression.clone(),
                        reading: reading.clone(),
                        rules: String::new(),
                        score: term.score,
                        hits: Vec::new(),
                    });
                if !rules.is_empty() {
                    if !raw.rules.is_empty() {
                        raw.rules.push(' ');
                    }
                    raw.rules.push_str(rules);
                }
                raw.score = raw.score.max(term.score);
                raw.hits.push(Hit {
                    dictionary: i,
                    id: term.id,
                    redirect: term.redirect,
                });
            }
        }
        Ok(merged.into_values().collect())
    }

    fn frequencies(
        &self,
        readers: &[Box<dyn DictReader + '_>],
        entry: &Entry,
    ) -> Result<Vec<Frequencies>, StoreError> {
        let mut out = Vec::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            if !d.store.info().kinds.frequency {
                continue;
            }
            let reading = entry.reading_or_expression();
            let values = dedup(
                readers[i]
                    .term_meta(&entry.expression)?
                    .into_iter()
                    .filter_map(|m| match m {
                        Meta::Frequency(f) if f.reading.as_deref().is_none_or(|r| r == reading) => {
                            Some(f)
                        }
                        _ => None,
                    }),
            );
            if !values.is_empty() {
                out.push(Frequencies {
                    dictionary: i,
                    values,
                });
            }
        }
        Ok(out)
    }

    fn pitches(
        &self,
        readers: &[Box<dyn DictReader + '_>],
        entry: &Entry,
    ) -> Result<Vec<Pitches>, StoreError> {
        let mut out = Vec::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            if !d.store.info().kinds.pitch {
                continue;
            }
            let reading = entry.reading_or_expression();
            let mut group = Pitches {
                dictionary: i,
                pitches: Vec::new(),
                ipa: Vec::new(),
            };
            for meta in readers[i].term_meta(&entry.expression)? {
                match meta {
                    Meta::Pitch {
                        reading: r,
                        pitches,
                    } if r.is_empty() || r == reading => {
                        for p in pitches {
                            // the same accent listed by two records of the dictionary (BUG-2152)
                            let same = |s: &Pitch| {
                                (&s.position, &s.nasal, &s.devoice)
                                    == (&p.position, &p.nasal, &p.devoice)
                            };
                            if !group.pitches.iter().any(same) {
                                group.pitches.push(p);
                            }
                        }
                    }
                    Meta::Ipa {
                        reading: r,
                        transcriptions,
                    } if r.is_empty() || r == reading => {
                        for t in transcriptions {
                            if !group.ipa.iter().any(|s| s.ipa == t.ipa) {
                                group.ipa.push(t);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !group.pitches.is_empty() || !group.ipa.is_empty() {
                out.push(group);
            }
        }
        Ok(out)
    }

    /// The sort key of an entry for one frequency dictionary (LKP-034 (5)):
    /// its lowest value ≥ 0, or the highest negated for occurrence-based
    /// dictionaries (DIC-003); missing values last.
    fn frequency_key(&self, entry: &Entry, dictionary: usize) -> i64 {
        let occurrence = self.dictionaries[dictionary]
            .store
            .info()
            .index
            .frequency_mode
            == FrequencyMode::OccurrenceBased;
        let values = entry
            .frequencies
            .iter()
            .filter(|f| f.dictionary == dictionary)
            .flat_map(|f| f.values.iter().map(|v| v.value))
            .filter(|&v| v >= 0);
        let best = if occurrence {
            values.max().map(|v| -v)
        } else {
            values.min()
        };
        best.unwrap_or(i64::MAX)
    }

    /// Keys 1–4 of LKP-034: longest match, fewer preprocessor steps, shorter
    /// trace, headword equal to the form looked up.
    fn compare_head(&self, a: &Entry, b: &Entry) -> Ordering {
        let len = |e: &Entry| e.matched.chars().count();
        let exact = |e: &Entry| e.expression == e.deinflected;
        len(b)
            .cmp(&len(a))
            .then(a.preprocessor_steps.cmp(&b.preprocessor_steps))
            .then(a.trace.len().cmp(&b.trace.len()))
            .then(exact(b).cmp(&exact(a)))
    }

    /// The order of LKP-034; `ka` and `kb` are the frequency keys.
    fn compare(&self, a: &Candidate, b: &Candidate, ka: &[i64], kb: &[i64]) -> Ordering {
        let priority = |c: &Candidate| c.hits.first().map_or(usize::MAX, |h| h.dictionary);
        let kana = |c: &Candidate| c.entry.expression == c.entry.reading;
        let (ea, eb) = (&a.entry, &b.entry);
        self.compare_head(ea, eb)
            .then_with(|| ka.cmp(kb))
            .then(priority(a).cmp(&priority(b)))
            .then(eb.score.total_cmp(&ea.score))
            .then(kana(b).cmp(&kana(a)))
    }
}

/// The values without repeats, in order.
fn dedup(values: impl Iterator<Item = Frequency>) -> Vec<Frequency> {
    let mut out: Vec<Frequency> = Vec::new();
    for v in values {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

fn kansai(trace: &[Step]) -> bool {
    trace.iter().any(|s| &*s.name == KANSAI)
}

/// Whether one of the protected words follows byte `at` of the window.
fn protected(window: &str, at: usize) -> bool {
    window
        .get(at..)
        .is_some_and(|rest| KANSAI_PROTECTED.iter().any(|p| rest.starts_with(p)))
}

impl Run<'_> {
    fn raw(&mut self, text: &str) -> Result<Arc<Vec<Raw>>, StoreError> {
        if let Some(hit) = self.raw.get(text) {
            return Ok(Arc::clone(hit));
        }
        let found = Arc::new(self.lookup.query(&self.readers, text)?);
        self.raw.insert(text.to_string(), Arc::clone(&found));
        Ok(found)
    }

    /// The readings the term dictionaries give a kanji group, the most
    /// attested first (LKP-031).
    fn run_readings(&mut self, run: &str) -> Result<Vec<String>, StoreError> {
        if let Some(hit) = self.run_readings.get(run) {
            return Ok(hit.clone());
        }
        let mut votes: Vec<(String, usize)> = Vec::new();
        for raw in self.raw(run)?.iter() {
            if raw.expression == run && !raw.reading.is_empty() {
                let reading = mixed::to_hiragana(&raw.reading);
                match votes.iter_mut().find(|(r, _)| *r == reading) {
                    Some((_, n)) => *n += raw.hits.len(),
                    None => votes.push((reading, raw.hits.len())),
                }
            }
        }
        votes.sort_by(|a, b| a.0.cmp(&b.0));
        votes.sort_by_key(|v| std::cmp::Reverse(v.1));
        let readings: Vec<String> = votes.into_iter().map(|(r, _)| r).collect();
        self.run_readings.insert(run.to_string(), readings.clone());
        Ok(readings)
    }

    fn scan(&mut self) -> Result<(), StoreError> {
        let lookup = self.lookup;
        for search in candidates(self.window, SCAN_LENGTH) {
            for variant in variants(search, &lookup.groups) {
                let mut targets: Vec<(String, Vec<Step>)> = Vec::new();
                for (n, d) in lookup.deinflectors.iter().enumerate() {
                    for x in d.deinflect(&variant.text) {
                        // the text itself once, not once per language
                        if n > 0 && x.trace.is_empty() {
                            continue;
                        }
                        let form = Form {
                            text: &x.text,
                            conditions: x.conditions,
                            deinflector: n,
                            trace: x
                                .trace
                                .iter()
                                .map(|&t| lookup.steps[n][usize::from(t)].clone())
                                .collect(),
                        };
                        self.form(search, variant.steps, &form, &mut targets)?;
                    }
                }
                if lookup.deinflectors.is_empty() {
                    let form = Form {
                        text: &variant.text,
                        conditions: 0,
                        deinflector: 0,
                        trace: Vec::new(),
                    };
                    self.form(search, variant.steps, &form, &mut targets)?;
                }
                // redirect targets: no part-of-speech filter, one level only
                for (form_of, trace) in targets {
                    let found = self.raw(&form_of)?;
                    let terms: Vec<&Raw> = found.iter().collect();
                    self.merge(search, variant.steps, &form_of, &trace, &terms, None)?;
                }
            }
        }
        Ok(())
    }

    fn form(
        &mut self,
        search: &str,
        steps: u32,
        form: &Form<'_>,
        targets: &mut Vec<(String, Vec<Step>)>,
    ) -> Result<(), StoreError> {
        let found = self.raw(form.text)?;
        let terms = self.filter(found.iter(), form);
        self.merge(search, steps, form.text, &form.trace, &terms, Some(targets))?;
        // Korean rules work on jamo; the dictionaries index syllables (BUG-2148)
        let syllables = nikomiru_text::hangul::reassemble(form.text);
        if syllables != form.text {
            let found = self.raw(&syllables)?;
            let terms = self.filter(found.iter(), form);
            self.merge(
                search,
                steps,
                &syllables,
                &form.trace,
                &terms,
                Some(targets),
            )?;
        }
        if mixed::is_mixed(form.text) {
            let text = form.text;
            let mut failed = None;
            let candidates = mixed::reading_candidates(text, |run| {
                self.run_readings(run).unwrap_or_else(|e| {
                    failed = Some(e);
                    Vec::new()
                })
            });
            if let Some(e) = failed {
                return Err(e);
            }
            for candidate in candidates {
                let found = self.raw(&candidate)?;
                let terms = self.filter(
                    found
                        .iter()
                        .filter(|r| mixed::matches(text, &r.expression, &r.reading)),
                    form,
                );
                if !terms.is_empty() {
                    self.merge(search, steps, text, &form.trace, &terms, Some(targets))?;
                }
            }
        }
        Ok(())
    }

    /// The part-of-speech filter (LKP-023).
    fn filter<'r>(&self, terms: impl Iterator<Item = &'r Raw>, form: &Form<'_>) -> Vec<&'r Raw> {
        let d = self.lookup.deinflectors.get(form.deinflector);
        terms
            .filter(|t| {
                form.conditions == 0
                    || d.is_none_or(|d| d.rules_conditions(&t.rules) & form.conditions != 0)
            })
            .collect()
    }

    /// Adds hits to the results (LKP-032, LKP-033). Records are copied only
    /// when they win.
    fn merge(
        &mut self,
        search: &str,
        steps: u32,
        deinflected: &str,
        trace: &[Step],
        terms: &[&Raw],
        mut targets: Option<&mut Vec<(String, Vec<Step>)>>,
    ) -> Result<(), StoreError> {
        for raw in terms {
            let hits: Cow<'_, [Hit]> = if raw.hits.iter().any(|h| h.redirect) {
                let mut kept = Vec::with_capacity(raw.hits.len());
                for h in &raw.hits {
                    if !h.redirect {
                        kept.push(*h);
                        continue;
                    }
                    let Some(targets) = targets.as_deref_mut() else {
                        continue;
                    };
                    let Some(record) = self.readers[h.dictionary].term(h.id)? else {
                        continue;
                    };
                    for (form_of, rules) in parse_redirects(&record.glossary()?).unwrap_or_default()
                    {
                        if form_of != raw.expression {
                            let mut chained = trace.to_vec();
                            chained.extend(rules.into_iter().map(|name| Step {
                                name: name.into(),
                                description: "".into(),
                            }));
                            targets.push((form_of, chained));
                        }
                    }
                }
                Cow::Owned(kept)
            } else {
                Cow::Borrowed(&raw.hits)
            };
            if hits.is_empty() {
                continue;
            }
            let key = key_of(&raw.expression, &raw.reading);
            let wins = match self.results.get(&key) {
                None => true,
                Some(Candidate { entry: held, .. }) => {
                    // the Kansai -た must not swallow ため/たび (BUG-3212)
                    if trace.is_empty()
                        && held.matched.len() > search.len()
                        && kansai(&held.trace)
                        && protected(self.window, search.len())
                    {
                        true
                    } else if held.trace.is_empty()
                        && search.len() > held.matched.len()
                        && kansai(trace)
                        && protected(self.window, held.matched.len())
                    {
                        false
                    } else {
                        let incoming = search.chars().count();
                        let current = held.matched.chars().count();
                        incoming > current
                            || (incoming == current && steps < held.preprocessor_steps)
                    }
                }
            };
            if wins {
                self.results.insert(
                    key,
                    Candidate {
                        entry: Entry {
                            expression: raw.expression.clone(),
                            reading: raw.reading.clone(),
                            matched: search.to_string(),
                            deinflected: deinflected.to_string(),
                            trace: trace.to_vec(),
                            preprocessor_steps: steps,
                            rules: raw.rules.clone(),
                            score: raw.score,
                            glossaries: Vec::new(),
                            frequencies: Vec::new(),
                            pitches: Vec::new(),
                        },
                        hits: hits.into_owned(),
                    },
                );
            }
        }
        Ok(())
    }

    fn finish(self, options: Options) -> Result<Vec<Entry>, StoreError> {
        let lookup = self.lookup;
        let mut groups: Vec<Candidate> = self.results.into_values().collect();
        // Keys 1–4 of LKP-034 do not need frequencies: whatever is behind the
        // last group of the budget on those keys cannot be shown.
        groups.sort_by(|a, b| lookup.compare_head(&a.entry, &b.entry));
        if let Some(cut) = groups.get(options.max_results.saturating_sub(1)).cloned() {
            let end = groups.partition_point(|c| {
                lookup.compare_head(&c.entry, &cut.entry) != Ordering::Greater
            });
            groups.truncate(end);
        }
        // frequencies before sorting, on the merged set (BUG-1304)
        for c in &mut groups {
            c.entry.frequencies = lookup.frequencies(&self.readers, &c.entry)?;
        }
        let frequency_dicts: Vec<usize> = (0..lookup.dictionaries.len())
            .filter(|&i| lookup.dictionaries[i].store.info().kinds.frequency)
            .collect();
        let mut ranked: Vec<(Vec<i64>, Candidate)> = groups
            .into_iter()
            .map(|c| {
                (
                    frequency_dicts
                        .iter()
                        .map(|&d| lookup.frequency_key(&c.entry, d))
                        .collect(),
                    c,
                )
            })
            .collect();
        ranked.sort_by(|(ka, a), (kb, b)| lookup.compare(a, b, ka, kb));
        let mut entries = Vec::with_capacity(options.max_results);
        for (_, Candidate { mut entry, hits }) in ranked.into_iter().take(options.max_results) {
            // the records of what is shown, and only those (LKP-035)
            for h in hits {
                if let Some(term) = self.readers[h.dictionary].term(h.id)? {
                    entry.glossaries.push(Glossary {
                        dictionary: h.dictionary,
                        term,
                    });
                }
            }
            // pitch accents only for what is shown (BUG-1304)
            entry.pitches = lookup.pitches(&self.readers, &entry)?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LKP-038
    #[test]
    fn the_cache_forgets_the_least_recently_used() {
        let mut lru = Lru::new(2);
        lru.put("a".into(), 1);
        lru.put("b".into(), 2);
        assert_eq!(lru.get("a"), Some(1));
        lru.put("c".into(), 3);
        assert_eq!(lru.get("b"), None, "b was the oldest");
        assert_eq!((lru.get("a"), lru.get("c")), (Some(1), Some(3)));
        lru.put("a".into(), 4);
        assert_eq!(lru.get("a"), Some(4));
        assert_eq!(lru.map.len(), 2);
        assert_eq!(lru.order.len(), 2);
    }
}
