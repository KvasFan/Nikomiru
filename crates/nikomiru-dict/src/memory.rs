// SPDX-License-Identifier: GPL-3.0-only

//! A [`DictStore`] in memory, for the L1 lookup tests: dictionaries written as
//! a few rows, and a count of the queries (a spy, T-DIC-032).

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::store::{
    Counts, DictInfo, DictReader, DictStore, Field, StoreError, StoredTerm, TermHead,
};
use crate::yomitan::{Index, Kanji, Kinds, Meta, Tag, Term};

/// A dictionary held in memory.
#[derive(Debug)]
pub struct MemStore {
    info: DictInfo,
    terms: Vec<StoredTerm>,
    meta: Vec<(String, Meta)>,
    kanji: Vec<Kanji>,
    kanji_meta: Vec<(String, Meta)>,
    tags: Vec<Tag>,
    queries: AtomicUsize,
}

impl MemStore {
    /// An empty dictionary called `title`.
    pub fn new(title: &str) -> Self {
        let index = Index {
            title: title.into(),
            revision: "1".into(),
            format: 3,
            ..Index::default()
        };
        Self {
            info: DictInfo {
                index,
                kinds: Kinds::default(),
                counts: Counts::default(),
                styles: String::new(),
            },
            terms: Vec::new(),
            meta: Vec::new(),
            kanji: Vec::new(),
            kanji_meta: Vec::new(),
            tags: Vec::new(),
            queries: AtomicUsize::new(0),
        }
    }

    /// Changes the index (language, frequency mode…).
    pub fn with_index(mut self, edit: impl FnOnce(&mut Index)) -> Self {
        edit(&mut self.info.index);
        self
    }

    /// Adds terms.
    pub fn with_terms(mut self, terms: Vec<Term>) -> Self {
        for t in terms {
            if !t.rules.trim().is_empty() {
                self.info.counts.with_rules += 1;
            }
            let id = self.terms.len() as u64;
            self.terms.push(StoredTerm::new(id, t));
        }
        self.info.counts.terms = self.terms.len() as u64;
        self.info.kinds.terms = !self.terms.is_empty();
        self
    }

    /// Adds term meta.
    pub fn with_meta(mut self, meta: Vec<(String, Meta)>) -> Self {
        for (_, m) in &meta {
            match m {
                Meta::Frequency(_) => {
                    self.info.kinds.frequency = true;
                    self.info.counts.frequencies += 1;
                }
                Meta::Pitch { .. } | Meta::Ipa { .. } => {
                    self.info.kinds.pitch = true;
                    self.info.counts.pitches += 1;
                }
            }
        }
        self.meta.extend(meta);
        self.info.counts.term_meta = self.meta.len() as u64;
        self
    }

    /// Adds kanji and kanji meta.
    pub fn with_kanji(mut self, kanji: Vec<Kanji>, meta: Vec<(String, Meta)>) -> Self {
        self.kanji.extend(kanji);
        self.kanji_meta.extend(meta);
        self.info.kinds.kanji = !self.kanji.is_empty();
        self.info.counts.kanji = self.kanji.len() as u64;
        self.info.counts.kanji_meta = self.kanji_meta.len() as u64;
        self
    }

    /// Adds tags.
    pub fn with_tags(mut self, tags: Vec<Tag>) -> Self {
        self.tags.extend(tags);
        self.info.counts.tags = self.tags.len() as u64;
        self
    }

    /// How many queries reached this dictionary.
    pub fn queries(&self) -> usize {
        self.queries.load(Ordering::Relaxed)
    }

    fn count(&self) {
        self.queries.fetch_add(1, Ordering::Relaxed);
    }
}

impl DictStore for MemStore {
    fn info(&self) -> &DictInfo {
        &self.info
    }

    fn reader(&self) -> Result<Box<dyn DictReader + '_>, StoreError> {
        Ok(Box::new(MemReader(self)))
    }

    fn tag(&self, name: &str) -> Result<Option<Tag>, StoreError> {
        Ok(self.tags.iter().find(|t| t.name == name).cloned())
    }

    fn media(&self, _path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(None)
    }
}

struct MemReader<'a>(&'a MemStore);

impl DictReader for MemReader<'_> {
    fn heads(&self, field: Field, text: &str) -> Result<Vec<TermHead>, StoreError> {
        self.0.count();
        Ok(self
            .0
            .terms
            .iter()
            .filter(|t| match field {
                Field::Expression => t.expression == text,
                Field::Reading => {
                    !t.reading.is_empty() && t.reading != t.expression && t.reading == text
                }
            })
            .map(TermHead::of)
            .collect())
    }

    fn term(&self, id: u64) -> Result<Option<StoredTerm>, StoreError> {
        Ok(usize::try_from(id)
            .ok()
            .and_then(|i| self.0.terms.get(i))
            .cloned())
    }

    fn term_meta(&self, text: &str) -> Result<Vec<Meta>, StoreError> {
        self.0.count();
        Ok(self
            .0
            .meta
            .iter()
            .filter(|(t, _)| t == text)
            .map(|(_, m)| m.clone())
            .collect())
    }

    fn kanji(&self, character: &str) -> Result<Option<Kanji>, StoreError> {
        self.0.count();
        Ok(self
            .0
            .kanji
            .iter()
            .find(|k| k.character == character)
            .cloned())
    }

    fn kanji_meta(&self, character: &str) -> Result<Vec<Meta>, StoreError> {
        self.0.count();
        Ok(self
            .0
            .kanji_meta
            .iter()
            .filter(|(t, _)| t == character)
            .map(|(_, m)| m.clone())
            .collect())
    }
}
