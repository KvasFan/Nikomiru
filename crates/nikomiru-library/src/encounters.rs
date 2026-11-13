// SPDX-License-Identifier: GPL-3.0-only

//! Historical new words encountered during playback (KNW-018). Each media
//! uid has a set of headword/reading keys with their first encounter time.
//! Status changes do not rewrite history. This additive table leaves every
//! existing record and the schema version unchanged (ADR 0007 §26).

use crate::db::{Library, LibraryError, db_err};
use crate::words::word_key;
use redb::{ReadableTable, TableDefinition};
use std::collections::BTreeMap;

/// First encounter time (Unix ms), by headword/reading key.
pub type Encounters = BTreeMap<String, i64>;

pub(crate) const ENCOUNTERS: TableDefinition<&str, &[u8]> = TableDefinition::new("word_encounters");

/// Union a media's encounter history, keeping the earliest timestamp.
pub fn merge(into: &mut Encounters, from: &Encounters) {
    for (word, at) in from {
        into.entry(word.clone())
            .and_modify(|ours| *ours = (*ours).min(*at))
            .or_insert(*at);
    }
}

impl Library {
    /// Record words that were unknown when actively encountered, atomically.
    /// Repeated visits and asynchronous writes preserve the earliest time.
    pub fn record_encounters(
        &self,
        uid: &str,
        words: &[(String, String)],
        at_ms: i64,
    ) -> Result<(), LibraryError> {
        if words.is_empty() {
            return Ok(());
        }
        let tx = self.write()?;
        {
            let mut table = db_err!(tx.open_table(ENCOUNTERS))?;
            let mut seen: Encounters = match db_err!(table.get(uid))? {
                Some(value) => postcard::from_bytes(value.value())?,
                None => BTreeMap::new(),
            };
            for (expression, reading) in words {
                if !expression.trim().is_empty() {
                    seen.entry(word_key(expression, reading))
                        .and_modify(|at| *at = (*at).min(at_ms))
                        .or_insert(at_ms);
                }
            }
            db_err!(table.insert(uid, postcard::to_stdvec(&seen)?.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(())
    }

    /// Encounter sets by media uid. Older or read-only libraries without
    /// this additive table have no history; no table is created by a read.
    pub fn word_encounters(&self) -> Result<BTreeMap<String, Encounters>, LibraryError> {
        self.read(|tx| match tx.open_table(ENCOUNTERS) {
            Ok(table) => {
                let mut out = BTreeMap::new();
                for row in db_err!(table.iter())? {
                    let (uid, value) = db_err!(row)?;
                    out.insert(
                        uid.value().to_string(),
                        postcard::from_bytes(value.value())?,
                    );
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(BTreeMap::new()),
            Err(error) => Err(LibraryError::Db(error.into())),
        })
    }

    /// Media uid, unique new words and latest first-encounter time, most
    /// recent first. The caller supplies a bound for the displayed history.
    pub fn encounter_stats(&self, limit: usize) -> Result<Vec<(String, usize, i64)>, LibraryError> {
        let mut rows: Vec<_> = self
            .word_encounters()?
            .into_iter()
            .filter_map(|(uid, words)| {
                let latest = words.values().copied().max()?;
                Some((uid, words.len(), latest))
            })
            .collect();
        rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::{WordMark, WordStatus};

    // T-KNW-018: unique headword/readings per media, history survives statuses
    // and restart; reads of an old database do not require an existing table.
    #[test]
    fn encounters_are_distinct_persistent_and_historical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.redb");
        {
            let lib = Library::open(&path).unwrap();
            assert!(lib.word_encounters().unwrap().is_empty());
            let words = vec![
                ("猫".into(), "ねこ".into()),
                ("猫".into(), "ねこ".into()),
                ("猫".into(), "びょう".into()),
            ];
            lib.record_encounters("video/a", &words, 20).unwrap();
            lib.record_encounters("video/a", &words[..1], 30).unwrap();
            lib.record_encounters("video/a", &words[..1], 10).unwrap();
            lib.record_encounters("video/b", &words[..1], 40).unwrap();
            lib.set_word_status(
                "猫",
                "ねこ",
                WordMark {
                    status: WordStatus::Known,
                    changed_at: 50,
                },
            )
            .unwrap();
            assert_eq!(
                lib.word_encounters().unwrap()["video/a"][&word_key("猫", "ねこ")],
                10
            );
            assert_eq!(lib.encounter_stats(1).unwrap(), [("video/b".into(), 1, 40)]);
            assert_eq!(
                lib.encounter_stats(10).unwrap()[1],
                ("video/a".into(), 2, 20)
            );
        }
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.word_encounters().unwrap()["video/a"].len(), 2);
        assert_eq!(lib.word_encounters().unwrap()["video/b"].len(), 1);
    }

    // T-KNW-018: the same word on two devices is one historical encounter,
    // with a stable earliest timestamp irrespective of merge order.
    #[test]
    fn encounter_union_is_commutative_and_idempotent() {
        let a = BTreeMap::from([("a".into(), 20), ("b".into(), 10)]);
        let b = BTreeMap::from([("a".into(), 5), ("c".into(), 30)]);
        let mut ab = a.clone();
        merge(&mut ab, &b);
        let mut ba = b.clone();
        merge(&mut ba, &a);
        assert_eq!(ab, ba);
        assert_eq!(ab["a"], 5);
        merge(&mut ba, &ab);
        assert_eq!(ab, ba);
    }
}
