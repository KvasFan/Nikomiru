// SPDX-License-Identifier: GPL-3.0-only

//! The cards made in Anki (ANK-039), in the `mined_cards` table of
//! `library.redb`: for statistics, and to mark words already mined later on.
//! A new table leaves the other records as they are, so the schema version
//! does not change (ADR 0007 §26).

use redb::{ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError};

const MINED_CARDS: TableDefinition<u64, &[u8]> = TableDefinition::new("mined_cards");

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
        lib.put_mined_card(7, &card("食べました")).unwrap();
        assert_eq!(lib.mined_card(7).unwrap(), Some(card("食べました")));
        lib.put_mined_card(7, &card("また食べた")).unwrap();
        assert_eq!(lib.mined_card(7).unwrap(), Some(card("また食べた")));
        assert_eq!(lib.mined_card_count().unwrap(), 1);
        assert_eq!(lib.schema(), crate::db::SCHEMA_VERSION, "no migration");
    }
}
