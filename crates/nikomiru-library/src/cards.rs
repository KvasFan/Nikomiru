// SPDX-License-Identifier: GPL-3.0-only

//! The cards made in Anki (ANK-039), in the `mined_cards` table of
//! `library.redb`: for statistics, and to mark words already mined later on.
//! A new table leaves the other records as they are, so the schema version
//! does not change (ADR 0007 §26).

use std::collections::HashSet;

use redb::{ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError};

pub(crate) const MINED_CARDS: TableDefinition<u64, &[u8]> = TableDefinition::new("mined_cards");

/// A card made from a scene (SPECIFICATION §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinedCard {
    /// Headword.
    pub expression: String,
    /// Its reading.
    pub reading: String,
    /// The sentence, as plain text.
    pub sentence: String,
    /// The video's uid (DAT-003).
    pub video_uid: String,
    /// Start of the clip, ms.
    pub start_ms: u32,
    /// End of the clip, ms.
    pub end_ms: u32,
    /// The id in the card's link (ANK-050).
    pub source_id: String,
    /// When it was made or last overwritten, Unix ms.
    pub created_at: i64,
}

impl Library {
    /// Records the card with Anki's note id `note_id`, replacing an earlier
    /// record of the same note (an overwrite, ANK-037).
    pub fn put_mined_card(&self, note_id: u64, card: &MinedCard) -> Result<(), LibraryError> {
        let bytes = postcard::to_stdvec(card)?;
        let tx = self.write()?;
        {
            let mut t = tx
                .open_table(MINED_CARDS)
                .map_err(|e| LibraryError::Db(e.into()))?;
            t.insert(note_id, bytes.as_slice())
                .map_err(|e| LibraryError::Db(e.into()))?;
        }
        tx.commit().map_err(|e| LibraryError::Db(e.into()))
    }

    /// Forgets the card of `note_id`, deleted from Anki (ANK-054); it does
    /// not come back from another device.
    pub fn delete_mined_card(&self, note_id: u64) -> Result<Option<MinedCard>, LibraryError> {
        let tx = self.write()?;
        let removed = {
            let mut t = tx
                .open_table(MINED_CARDS)
                .map_err(|e| LibraryError::Db(e.into()))?;
            let removed = t.remove(note_id).map_err(|e| LibraryError::Db(e.into()))?;
            match removed {
                Some(v) => Some(postcard::from_bytes::<MinedCard>(v.value())?),
                None => None,
            }
        };
        if removed.is_some() {
            crate::sync::bury(&tx, &crate::sync::card_key(note_id))?;
        }
        tx.commit().map_err(|e| LibraryError::Db(e.into()))?;
        Ok(removed)
    }

    /// The card recorded for `note_id`.
    pub fn mined_card(&self, note_id: u64) -> Result<Option<MinedCard>, LibraryError> {
        self.read(|tx| match tx.open_table(MINED_CARDS) {
            Ok(t) => match t.get(note_id).map_err(|e| LibraryError::Db(e.into()))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// The headwords of the cards made, to mark them in the subtitles
    /// (LKP-061).
    pub fn mined_expressions(&self) -> Result<HashSet<String>, LibraryError> {
        self.read(|tx| match tx.open_table(MINED_CARDS) {
            Ok(t) => {
                let mut out = HashSet::new();
                for row in t.iter().map_err(|e| LibraryError::Db(e.into()))? {
                    let (_, v) = row.map_err(|e| LibraryError::Db(e.into()))?;
                    let card: MinedCard = postcard::from_bytes(v.value())?;
                    out.insert(card.expression);
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(HashSet::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// How many cards were made.
    pub fn mined_card_count(&self) -> Result<u64, LibraryError> {
        self.read(|tx| match tx.open_table(MINED_CARDS) {
            Ok(t) => t.len().map_err(|e| LibraryError::Db(e.into())),
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(0),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(sentence: &str) -> MinedCard {
        MinedCard {
            expression: "食べる".into(),
            reading: "たべる".into(),
            sentence: sentence.into(),
            video_uid: "video/av_basic".into(),
            start_ms: 1_880,
            end_ms: 2_700,
            source_id: "12345678-1234-4234-8234-123456789abc".into(),
            created_at: 1_760_000_000_000,
        }
    }

    // T-ANK-039
    #[test]
    fn cards_are_recorded_and_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert_eq!(lib.mined_card(7).unwrap(), None, "no table yet");
        assert_eq!(lib.mined_card_count().unwrap(), 0);
        assert!(lib.mined_expressions().unwrap().is_empty());
        lib.put_mined_card(7, &card("食べました")).unwrap();
        assert_eq!(lib.mined_card(7).unwrap(), Some(card("食べました")));
        lib.put_mined_card(7, &card("また食べた")).unwrap();
        assert_eq!(lib.mined_card(7).unwrap(), Some(card("また食べた")));
        assert_eq!(lib.mined_card_count().unwrap(), 1);
        lib.put_mined_card(8, &card("食べる")).unwrap();
        // T-LKP-061 the headwords, once each
        assert_eq!(
            lib.mined_expressions().unwrap(),
            HashSet::from(["食べる".to_string()])
        );
        assert_eq!(lib.mined_card_count().unwrap(), 2);
        assert_eq!(lib.schema(), crate::db::SCHEMA_VERSION, "no migration");
    }

    // T-ANK-054: a card deleted from Anki is forgotten, here and on the
    // other devices
    #[test]
    fn a_card_undone_is_forgotten_everywhere() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let other = Library::open(&dir.path().join("other.redb")).unwrap();
        lib.put_mined_card(7, &card("食べました")).unwrap();
        other.put_mined_card(7, &card("食べました")).unwrap();
        assert_eq!(lib.delete_mined_card(7).unwrap(), Some(card("食べました")));
        assert_eq!(lib.delete_mined_card(7).unwrap(), None);
        assert!(lib.mined_expressions().unwrap().is_empty());
        let mut theirs = other.export_state(1).unwrap();
        let ours = lib.export_state(1).unwrap();
        assert!(ours.deleted.contains_key(&crate::sync::card_key(7)));
        theirs.merge(&ours);
        assert!(theirs.cards.is_empty());
        other.apply_state(&theirs).unwrap();
        assert_eq!(other.mined_card(7).unwrap(), None);
    }
}
