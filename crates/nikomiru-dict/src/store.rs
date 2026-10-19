// SPDX-License-Identifier: GPL-3.0-only

//! Dictionary storage (SPECIFICATION §6.5.3, DIC-020): one redb file per
//! dictionary behind the [`DictStore`] trait. Records are postcard; glossaries
//! are JSON compressed with lz4 and decompressed only for the entries that are
//! shown (LKP-035).

use std::path::Path;

use redb::{
    Database, MultimapTableDefinition, ReadableDatabase, TableDefinition, WriteTransaction,
};
use serde::{Deserialize, Serialize};

use crate::yomitan::{Index, Kanji, Kinds, Meta, Tag, Term};

/// Schema version of the dictionary files.
pub const SCHEMA: u32 = 1;

const INFO: TableDefinition<&str, &[u8]> = TableDefinition::new("info");
const TERMS: TableDefinition<u64, &[u8]> = TableDefinition::new("terms");
const BY_EXPRESSION: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("term_by_expression");
const BY_READING: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("term_by_reading");
const TERM_META: MultimapTableDefinition<&str, &[u8]> = MultimapTableDefinition::new("term_meta");
const KANJI: TableDefinition<&str, &[u8]> = TableDefinition::new("kanji");
const KANJI_META: MultimapTableDefinition<&str, &[u8]> = MultimapTableDefinition::new("kanji_meta");
const TAGS: TableDefinition<&str, &[u8]> = TableDefinition::new("tags");
const MEDIA: TableDefinition<&str, &[u8]> = TableDefinition::new("media");

/// Errors of the storage.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// redb failed.
    #[error("dictionary database: {0}")]
    Db(#[from] redb::Error),
    /// A record could not be decoded.
    #[error("dictionary record: {0}")]
    Record(#[from] postcard::Error),
    /// A glossary could not be decompressed.
    #[error("dictionary glossary: {0}")]
    Glossary(String),
    /// The file was written by another schema version.
    #[error("dictionary schema {0} is not supported")]
    Schema(u32),
}

impl nikomiru_core::Localized for StoreError {
    fn message_id(&self) -> &'static str {
        match self {
            StoreError::Db(_)
            | StoreError::Record(_)
            | StoreError::Glossary(_)
            | StoreError::Schema(_) => "error-dict-store",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        vec![("reason", self.to_string())]
    }
}

macro_rules! db {
    ($e:expr) => {
        $e.map_err(|e| StoreError::Db(e.into()))
    };
}

/// Entry counts of a dictionary (DIC-030).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Terms.
    pub terms: u64,
    /// Term meta (frequencies, pitch, IPA).
    pub term_meta: u64,
    /// Kanji.
    pub kanji: u64,
    /// Kanji meta.
    pub kanji_meta: u64,
    /// Tags.
    pub tags: u64,
    /// Media files.
    pub media: u64,
    /// Terms with deinflection rules: with none, empty rules mean `*`
    /// (LKP-023, BUG-2543).
    pub with_rules: u64,
}

/// What a dictionary file says about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictInfo {
    /// `index.json`.
    pub index: Index,
    /// What it contains.
    pub kinds: Kinds,
    /// Entry counts.
    pub counts: Counts,
    /// `styles.css`, if any (DIC-035).
    pub styles: String,
}

/// A term as stored: the glossary stays compressed until asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredTerm {
    /// Id inside the dictionary.
    pub id: u64,
    /// Headword.
    pub expression: String,
    /// Reading.
    pub reading: String,
    /// Definition tags, space separated.
    pub definition_tags: String,
    /// Deinflection rules, space separated.
    pub rules: String,
    /// Score.
    pub score: f64,
    /// Sequence number.
    pub sequence: i64,
    /// Term tags, space separated.
    pub term_tags: String,
    glossary_lz4: Vec<u8>,
}

impl StoredTerm {
    /// The record of `term`, with id `id`.
    pub fn new(id: u64, t: Term) -> Self {
        Self {
            id,
            expression: t.expression,
            reading: t.reading,
            definition_tags: t.definition_tags,
            rules: t.rules,
            score: t.score,
            sequence: t.sequence,
            term_tags: t.term_tags,
            glossary_lz4: lz4_flex::compress_prepend_size(t.glossary.as_bytes()),
        }
    }

    /// The glossary JSON.
    pub fn glossary(&self) -> Result<String, StoreError> {
        let bytes = lz4_flex::decompress_size_prepended(&self.glossary_lz4)
            .map_err(|e| StoreError::Glossary(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| StoreError::Glossary(e.to_string()))
    }

    /// Length of the glossary JSON in bytes, without decompressing it.
    pub fn glossary_len(&self) -> usize {
        self.glossary_lz4
            .first_chunk::<4>()
            .map_or(0, |n| u32::from_le_bytes(*n) as usize)
    }
}

/// Which index a term search uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The headword.
    Expression,
    /// The reading.
    Reading,
}

/// Read access to one dictionary (DIC-020).
pub trait DictStore: Send + Sync {
    /// What the dictionary says about itself.
    fn info(&self) -> &DictInfo;
    /// Terms whose `field` equals `text`.
    fn terms(&self, field: Field, text: &str) -> Result<Vec<StoredTerm>, StoreError>;
    /// Frequencies, pitch accents and IPA of `text`.
    fn term_meta(&self, text: &str) -> Result<Vec<Meta>, StoreError>;
    /// The kanji entry of `character`.
    fn kanji(&self, character: &str) -> Result<Option<Kanji>, StoreError>;
    /// Frequencies of `character`.
    fn kanji_meta(&self, character: &str) -> Result<Vec<Meta>, StoreError>;
    /// The tag called `name`.
    fn tag(&self, name: &str) -> Result<Option<Tag>, StoreError>;
    /// A media file (DIC-001, POP-010), by its normalized path.
    fn media(&self, path: &str) -> Result<Option<Vec<u8>>, StoreError>;
}

/// The normalized form of a media path: `/` separators, no leading `/`.
pub fn media_key(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

/// A dictionary file opened for reading.
#[derive(Debug)]
pub struct RedbStore {
    db: Database,
    info: DictInfo,
}

fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, StoreError> {
    Ok(postcard::from_bytes(bytes)?)
}

impl RedbStore {
    /// Opens a dictionary file.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let db = db!(Database::builder().set_cache_size(16 << 20).open(path))?;
        let info = {
            let txn = db!(db.begin_read())?;
            let table = db!(txn.open_table(INFO))?;
            let schema: u32 = match db!(table.get("schema"))? {
                Some(v) => decode(v.value())?,
                None => 0,
            };
            if schema != SCHEMA {
                return Err(StoreError::Schema(schema));
            }
            let info = db!(table.get("info"))?.ok_or(StoreError::Schema(0))?;
            decode::<DictInfo>(info.value())?
        };
        Ok(Self { db, info })
    }
}

impl DictStore for RedbStore {
    fn info(&self) -> &DictInfo {
        &self.info
    }

    fn terms(&self, field: Field, text: &str) -> Result<Vec<StoredTerm>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        let index = db!(txn.open_multimap_table(match field {
            Field::Expression => BY_EXPRESSION,
            Field::Reading => BY_READING,
        }))?;
        let terms = db!(txn.open_table(TERMS))?;
        let mut out = Vec::new();
        for id in db!(index.get(text))? {
            let id = db!(id)?.value();
            if let Some(bytes) = db!(terms.get(id))? {
                out.push(decode(bytes.value())?);
            }
        }
        Ok(out)
    }

    fn term_meta(&self, text: &str) -> Result<Vec<Meta>, StoreError> {
        multimap(&self.db, TERM_META, text)
    }

    fn kanji(&self, character: &str) -> Result<Option<Kanji>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        let table = db!(txn.open_table(KANJI))?;
        db!(table.get(character))?
            .map(|v| decode(v.value()))
            .transpose()
    }

    fn kanji_meta(&self, character: &str) -> Result<Vec<Meta>, StoreError> {
        multimap(&self.db, KANJI_META, character)
    }

    fn tag(&self, name: &str) -> Result<Option<Tag>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        let table = db!(txn.open_table(TAGS))?;
        db!(table.get(name))?.map(|v| decode(v.value())).transpose()
    }

    fn media(&self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        let table = db!(txn.open_table(MEDIA))?;
        Ok(db!(table.get(media_key(path).as_str()))?.map(|v| v.value().to_vec()))
    }
}

fn multimap(
    db: &Database,
    def: MultimapTableDefinition<&str, &[u8]>,
    key: &str,
) -> Result<Vec<Meta>, StoreError> {
    let txn = db!(db.begin_read())?;
    let table = db!(txn.open_multimap_table(def))?;
    let mut out = Vec::new();
    for v in db!(table.get(key))? {
        out.push(decode(db!(v)?.value())?);
    }
    Ok(out)
}

fn encode<T: Serialize>(v: &T) -> Result<Vec<u8>, StoreError> {
    Ok(postcard::to_allocvec(v)?)
}

/// Writes a new dictionary file in one transaction (DIC-013: the import
/// commits only at the end, and the folder is renamed only after that).
pub struct StoreWriter {
    db: Database,
    txn: Option<WriteTransaction>,
    next_id: u64,
    by_expression: Vec<(String, u64)>,
    by_reading: Vec<(String, u64)>,
    counts: Counts,
}

impl std::fmt::Debug for StoreWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreWriter")
            .field("counts", &self.counts)
            .finish()
    }
}

impl StoreWriter {
    /// Creates the file at `path`.
    pub fn create(path: &Path) -> Result<Self, StoreError> {
        let db = db!(Database::create(path))?;
        let txn = db!(db.begin_write())?;
        Ok(Self {
            db,
            txn: Some(txn),
            next_id: 0,
            by_expression: Vec::new(),
            by_reading: Vec::new(),
            counts: Counts::default(),
        })
    }

    fn txn(&self) -> Result<&WriteTransaction, StoreError> {
        self.txn
            .as_ref()
            .ok_or_else(|| StoreError::Glossary("writer already finished".into()))
    }

    /// Adds terms.
    pub fn add_terms(&mut self, terms: Vec<Term>) -> Result<(), StoreError> {
        let mut rows = Vec::with_capacity(terms.len());
        for t in terms {
            let id = self.next_id;
            self.next_id += 1;
            self.by_expression.push((t.expression.clone(), id));
            if !t.reading.is_empty() && t.reading != t.expression {
                self.by_reading.push((t.reading.clone(), id));
            }
            if !t.rules.trim().is_empty() {
                self.counts.with_rules += 1;
            }
            let record = StoredTerm::new(id, t);
            rows.push((id, encode(&record)?));
        }
        {
            let mut table = db!(self.txn()?.open_table(TERMS))?;
            for (id, bytes) in &rows {
                db!(table.insert(*id, bytes.as_slice()))?;
            }
        }
        self.counts.terms += rows.len() as u64;
        Ok(())
    }

    /// Adds term meta (`kanji = false`) or kanji meta.
    pub fn add_meta(&mut self, meta: Vec<(String, Meta)>, kanji: bool) -> Result<(), StoreError> {
        let n = meta.len() as u64;
        {
            let mut table =
                db!(self
                    .txn()?
                    .open_multimap_table(if kanji { KANJI_META } else { TERM_META }))?;
            for (key, m) in meta {
                db!(table.insert(key.as_str(), encode(&m)?.as_slice()))?;
            }
        }
        if kanji {
            self.counts.kanji_meta += n;
        } else {
            self.counts.term_meta += n;
        }
        Ok(())
    }

    /// Adds kanji entries.
    pub fn add_kanji(&mut self, kanji: Vec<Kanji>) -> Result<(), StoreError> {
        {
            let mut table = db!(self.txn()?.open_table(KANJI))?;
            for k in &kanji {
                db!(table.insert(k.character.as_str(), encode(k)?.as_slice()))?;
            }
        }
        self.counts.kanji += kanji.len() as u64;
        Ok(())
    }

    /// Adds tags (DIC-002).
    pub fn add_tags(&mut self, tags: Vec<Tag>) -> Result<(), StoreError> {
        {
            let mut table = db!(self.txn()?.open_table(TAGS))?;
            for t in &tags {
                db!(table.insert(t.name.as_str(), encode(t)?.as_slice()))?;
            }
        }
        self.counts.tags += tags.len() as u64;
        Ok(())
    }

    /// Adds a media file.
    pub fn add_media(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        {
            let mut table = db!(self.txn()?.open_table(MEDIA))?;
            db!(table.insert(media_key(path).as_str(), bytes))?;
        }
        self.counts.media += 1;
        Ok(())
    }

    /// The counts so far.
    pub fn counts(&self) -> Counts {
        self.counts
    }

    /// Writes the indexes (sorted by key, as fast as redb inserts) and the
    /// info, and commits.
    pub fn finish(
        mut self,
        index: Index,
        kinds: Kinds,
        styles: String,
    ) -> Result<DictInfo, StoreError> {
        let txn = self
            .txn
            .take()
            .ok_or_else(|| StoreError::Glossary("writer already finished".into()))?;
        for (def, mut pairs) in [
            (BY_EXPRESSION, std::mem::take(&mut self.by_expression)),
            (BY_READING, std::mem::take(&mut self.by_reading)),
        ] {
            pairs.sort();
            let mut table = db!(txn.open_multimap_table(def))?;
            for (key, id) in &pairs {
                db!(table.insert(key.as_str(), *id))?;
            }
        }
        // every table exists, even empty, so readers can open them
        db!(txn.open_table(TERMS))?;
        db!(txn.open_multimap_table(TERM_META))?;
        db!(txn.open_table(KANJI))?;
        db!(txn.open_multimap_table(KANJI_META))?;
        db!(txn.open_table(TAGS))?;
        db!(txn.open_table(MEDIA))?;
        let info = DictInfo {
            index,
            kinds,
            counts: self.counts,
            styles,
        };
        {
            let mut table = db!(txn.open_table(INFO))?;
            db!(table.insert("schema", encode(&SCHEMA)?.as_slice()))?;
            db!(table.insert("info", encode(&info)?.as_slice()))?;
        }
        db!(txn.commit())?;
        drop(self.db);
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yomitan::{Frequency, FrequencyMode};

    fn index() -> Index {
        Index {
            title: "t".into(),
            revision: "1".into(),
            format: 3,
            sequenced: false,
            frequency_mode: FrequencyMode::RankBased,
            source_language: None,
            target_language: None,
            author: None,
            attribution: None,
            description: None,
            url: None,
        }
    }

    fn term(expression: &str, reading: &str) -> Term {
        Term {
            expression: expression.into(),
            reading: reading.into(),
            definition_tags: "v1".into(),
            rules: "v1".into(),
            score: 1.0,
            glossary: r#"["to eat"]"#.into(),
            sequence: 7,
            term_tags: String::new(),
        }
    }

    #[test]
    fn written_entries_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dict.redb");
        let mut w = StoreWriter::create(&path).unwrap();
        w.add_terms(vec![
            term("食べる", "たべる"),
            term("喰べる", "たべる"),
            term("たべる", "たべる"),
        ])
        .unwrap();
        w.add_meta(
            vec![(
                "食べる".into(),
                Meta::Frequency(Frequency {
                    reading: None,
                    value: 100,
                    display: None,
                }),
            )],
            false,
        )
        .unwrap();
        w.add_media("\\img/a.png", &[1, 2, 3]).unwrap();
        let info = w
            .finish(index(), Kinds::default(), "a { color: red }".into())
            .unwrap();
        assert_eq!(info.counts.terms, 3);

        let s = RedbStore::open(&path).unwrap();
        assert_eq!(s.info().styles, "a { color: red }");
        let by_expr = s.terms(Field::Expression, "食べる").unwrap();
        assert_eq!(by_expr.len(), 1);
        assert_eq!(by_expr[0].glossary().unwrap(), r#"["to eat"]"#);
        assert_eq!(by_expr[0].glossary_len(), r#"["to eat"]"#.len());
        assert_eq!(by_expr[0].sequence, 7);
        // the reading index holds only readings that differ from the headword
        assert_eq!(s.terms(Field::Reading, "たべる").unwrap().len(), 2);
        assert_eq!(s.term_meta("食べる").unwrap().len(), 1);
        assert_eq!(s.media("img/a.png").unwrap(), Some(vec![1, 2, 3]));
        assert!(s.kanji("食").unwrap().is_none());
    }
}
