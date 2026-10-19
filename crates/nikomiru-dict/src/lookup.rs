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

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use nikomiru_text::scan::{SCAN_LENGTH, candidates};
use nikomiru_text::variants::{Group, groups_for_all, variants};

use crate::deinflect::Deinflector;
use crate::mixed;
use crate::store::{DictStore, Field, StoreError, StoredTerm};
use crate::yomitan::{Frequency, FrequencyMode, Ipa, Kanji, Meta, Pitch};

/// Glossaries this short may be a redirect (LKP-032).
pub const MAX_REDIRECT_BYTES: usize = 1024;

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

/// One step of a trace (LKP-024): a transform or a redirect's rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Name (`-て`).
    pub name: String,
    /// What it means (empty for redirects).
    pub description: String,
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
    /// Reading (may be empty: then the headword is its own reading).
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
    glossaries: Vec<Glossary>,
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
    window: &'a str,
    results: BTreeMap<Key, Entry>,
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
        Self {
            groups: groups_for_all(languages.iter().map(String::as_str)),
            deinflectors: languages
                .iter()
                .filter_map(|l| Deinflector::for_language(l))
                .collect(),
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
        let mut run = Run {
            lookup: self,
            window,
            results: BTreeMap::new(),
            raw: HashMap::new(),
            run_readings: HashMap::new(),
        };
        run.scan()?;
        let entries = Arc::new(run.finish(options)?);
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(key, Arc::clone(&entries));
        }
        Ok(entries)
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
    fn query(&self, text: &str) -> Result<Vec<Raw>, StoreError> {
        let mut merged: BTreeMap<Key, Raw> = BTreeMap::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            let info = d.store.info();
            if !info.kinds.terms {
                continue;
            }
            let mut terms = d.store.terms(Field::Expression, text)?;
            terms.extend(d.store.terms(Field::Reading, text)?);
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
                let raw = merged
                    .entry(key_of(&term.expression, &term.reading))
                    .or_insert_with(|| Raw {
                        expression: term.expression.clone(),
                        reading: term.reading.clone(),
                        rules: String::new(),
                        score: term.score,
                        glossaries: Vec::new(),
                    });
                if !rules.is_empty() {
                    if !raw.rules.is_empty() {
                        raw.rules.push(' ');
                    }
                    raw.rules.push_str(rules);
                }
                raw.score = raw.score.max(term.score);
                if raw.reading.is_empty() {
                    raw.reading.clone_from(&term.reading);
                }
                raw.glossaries.push(Glossary {
                    dictionary: i,
                    term,
                });
            }
        }
        Ok(merged.into_values().collect())
    }

    fn frequencies(&self, entry: &Entry) -> Result<Vec<Frequencies>, StoreError> {
        let mut out = Vec::new();
        for (i, d) in self.dictionaries.iter().enumerate() {
            if !d.store.info().kinds.frequency {
                continue;
            }
            let reading = entry.reading_or_expression();
            let values = dedup(
                d.store
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

    fn pitches(&self, entry: &Entry) -> Result<Vec<Pitches>, StoreError> {
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
            for meta in d.store.term_meta(&entry.expression)? {
                match meta {
                    Meta::Pitch {
                        reading: r,
                        pitches,
                    } if r == reading => {
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
                    } if r == reading => {
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

    /// The lowest frequency value of an entry in one dictionary, or the highest
    /// for occurrence-based dictionaries (DIC-003); `None` without one ≥ 0.
    fn frequency_rank(&self, entry: &Entry, dictionary: usize) -> Option<i64> {
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
        if occurrence {
            values.max()
        } else {
            values.min()
        }
    }

    /// The order of LKP-034.
    fn compare(&self, a: &Entry, b: &Entry, frequency_dicts: &[usize]) -> Ordering {
        let len = |e: &Entry| e.matched.chars().count();
        let exact = |e: &Entry| e.expression == e.deinflected;
        let priority = |e: &Entry| e.glossaries.first().map_or(usize::MAX, |g| g.dictionary);
        let kana = |e: &Entry| e.expression == e.reading;
        len(b)
            .cmp(&len(a))
            .then(a.preprocessor_steps.cmp(&b.preprocessor_steps))
            .then(a.trace.len().cmp(&b.trace.len()))
            .then(exact(b).cmp(&exact(a)))
            .then_with(|| {
                for &d in frequency_dicts {
                    let occurrence = self.dictionaries[d].store.info().index.frequency_mode
                        == FrequencyMode::OccurrenceBased;
                    let order = match (self.frequency_rank(a, d), self.frequency_rank(b, d)) {
                        (Some(x), Some(y)) if occurrence => y.cmp(&x),
                        (Some(x), Some(y)) => x.cmp(&y),
                        (Some(_), None) => Ordering::Less,
                        (None, Some(_)) => Ordering::Greater,
                        (None, None) => Ordering::Equal,
                    };
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                Ordering::Equal
            })
            .then(priority(a).cmp(&priority(b)))
            .then(b.score.total_cmp(&a.score))
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

/// `[formOf, [rules…]]`: a glossary item that points to another headword.
type Redirect = (String, Vec<String>);

/// The redirects of a glossary made only of `[formOf, [rules…]]` items
/// (LKP-032); `None` for anything else, a real definition included.
fn redirects(term: &StoredTerm) -> Result<Option<Vec<Redirect>>, StoreError> {
    if term.glossary_len() > MAX_REDIRECT_BYTES {
        return Ok(None);
    }
    let json = term.glossary()?;
    let mut rest = json.trim_start();
    for _ in 0..2 {
        let Some(after) = rest.strip_prefix('[') else {
            return Ok(None);
        };
        rest = after.trim_start();
    }
    let Ok(items) = serde_json::from_str::<Vec<Redirect>>(&json) else {
        return Ok(None);
    };
    if items.is_empty() || items.iter().any(|(form, _)| form.is_empty()) {
        return Ok(None);
    }
    Ok(Some(items))
}

fn kansai(trace: &[Step]) -> bool {
    trace.iter().any(|s| s.name == KANSAI)
}

impl Run<'_> {
    fn raw(&mut self, text: &str) -> Result<Arc<Vec<Raw>>, StoreError> {
        if let Some(hit) = self.raw.get(text) {
            return Ok(Arc::clone(hit));
        }
        let found = Arc::new(self.lookup.query(text)?);
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
                    Some((_, n)) => *n += raw.glossaries.len(),
                    None => votes.push((reading, raw.glossaries.len())),
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
                let mut forms: Vec<(String, u64, usize, Vec<u16>)> = Vec::new();
                for (n, d) in lookup.deinflectors.iter().enumerate() {
                    for x in d.deinflect(&variant.text) {
                        // the text itself once, not once per language
                        if n == 0 || !x.trace.is_empty() {
                            forms.push((x.text, x.conditions, n, x.trace));
                        }
                    }
                }
                if lookup.deinflectors.is_empty() {
                    forms.push((variant.text.clone(), 0, 0, Vec::new()));
                }
                let mut targets: Vec<(String, Vec<Step>)> = Vec::new();
                for (text, conditions, n, trace) in &forms {
                    let form = Form {
                        text,
                        conditions: *conditions,
                        deinflector: *n,
                        trace: trace
                            .iter()
                            .map(|&t| {
                                let t = lookup.deinflectors[*n].transform(t);
                                Step {
                                    name: t.name.clone(),
                                    description: t.description.clone(),
                                }
                            })
                            .collect(),
                    };
                    self.form(search, variant.steps, &form, &mut targets)?;
                }
                // redirect targets: no part-of-speech filter, one level only
                for (form_of, trace) in targets {
                    let terms: Vec<Raw> = self.raw(&form_of)?.iter().cloned().collect();
                    self.merge(search, variant.steps, &form_of, &trace, terms, None)?;
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
        let mut texts = vec![form.text.to_string()];
        // Korean rules work on jamo; the dictionaries index syllables (BUG-2148)
        let syllables = nikomiru_text::hangul::reassemble(form.text);
        if syllables != form.text {
            texts.push(syllables);
        }
        for text in &texts {
            let found = self.raw(text)?;
            let terms = self.filter(found.iter().cloned().collect(), form);
            self.merge(search, steps, text, &form.trace, terms, Some(targets))?;
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
                let terms: Vec<Raw> = self
                    .raw(&candidate)?
                    .iter()
                    .filter(|r| mixed::matches(text, &r.expression, &r.reading))
                    .cloned()
                    .collect();
                if terms.is_empty() {
                    continue;
                }
                let terms = self.filter(terms, form);
                self.merge(search, steps, text, &form.trace, terms, Some(targets))?;
            }
        }
        Ok(())
    }

    /// The part-of-speech filter (LKP-023).
    fn filter(&self, mut terms: Vec<Raw>, form: &Form<'_>) -> Vec<Raw> {
        if form.conditions != 0 {
            let d = &self.lookup.deinflectors[form.deinflector];
            terms.retain(|t| d.rules_conditions(&t.rules) & form.conditions != 0);
        }
        terms
    }

    fn merge(
        &mut self,
        search: &str,
        steps: u32,
        deinflected: &str,
        trace: &[Step],
        terms: Vec<Raw>,
        mut targets: Option<&mut Vec<(String, Vec<Step>)>>,
    ) -> Result<(), StoreError> {
        for mut raw in terms {
            let mut kept = Vec::with_capacity(raw.glossaries.len());
            for g in raw.glossaries {
                let Some(found) = redirects(&g.term)? else {
                    kept.push(g);
                    continue;
                };
                if let Some(targets) = targets.as_deref_mut() {
                    for (form_of, rules) in found {
                        if form_of != raw.expression {
                            let mut chained = trace.to_vec();
                            chained.extend(rules.into_iter().map(|name| Step {
                                name,
                                description: String::new(),
                            }));
                            targets.push((form_of, chained));
                        }
                    }
                }
            }
            raw.glossaries = kept;
            if raw.glossaries.is_empty() {
                continue;
            }
            let entry = Entry {
                expression: raw.expression,
                reading: raw.reading,
                matched: search.to_string(),
                deinflected: deinflected.to_string(),
                trace: trace.to_vec(),
                preprocessor_steps: steps,
                rules: raw.rules,
                score: raw.score,
                glossaries: raw.glossaries,
                frequencies: Vec::new(),
                pitches: Vec::new(),
            };
            let key = key_of(&entry.expression, &entry.reading);
            match self.results.get_mut(&key) {
                None => {
                    self.results.insert(key, entry);
                }
                Some(held) => {
                    // the Kansai -た must not swallow ため/たび (BUG-3212)
                    let window = self.window;
                    let short_wins = |short: &Entry, long: &Entry| {
                        short.trace.is_empty()
                            && long.matched.len() > short.matched.len()
                            && kansai(&long.trace)
                            && window.get(short.matched.len()..).is_some_and(|rest| {
                                KANSAI_PROTECTED.iter().any(|p| rest.starts_with(p))
                            })
                    };
                    if short_wins(&entry, held) {
                        *held = entry;
                        continue;
                    }
                    if short_wins(held, &entry) {
                        continue;
                    }
                    let incoming = entry.matched.chars().count();
                    let current = held.matched.chars().count();
                    if incoming > current
                        || (incoming == current && steps < held.preprocessor_steps)
                    {
                        *held = entry;
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(self, options: Options) -> Result<Vec<Entry>, StoreError> {
        let lookup = self.lookup;
        let mut entries: Vec<Entry> = self.results.into_values().collect();
        // frequencies before sorting, on the merged set (BUG-1304)
        for e in &mut entries {
            e.frequencies = lookup.frequencies(e)?;
        }
        let frequency_dicts: Vec<usize> = (0..lookup.dictionaries.len())
            .filter(|&i| lookup.dictionaries[i].store.info().kinds.frequency)
            .collect();
        entries.sort_by(|a, b| lookup.compare(a, b, &frequency_dicts));
        entries.truncate(options.max_results);
        // pitch accents only for what is shown (BUG-1304)
        for e in &mut entries {
            e.pitches = lookup.pitches(e)?;
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
