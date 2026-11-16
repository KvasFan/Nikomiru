// SPDX-License-Identifier: GPL-3.0-only

//! Durable write-ahead batch operation identities and outcomes (BMG-004).
use crate::{Library, LibraryError, db::db_err};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
const JOURNALS: TableDefinition<&str, &[u8]> = TableDefinition::new("mining_batches");
/// Maximum retained batches; overflow requires explicit cleanup of resolved journals.
pub const MAX_BATCHES: usize = 100;
/// Maximum cards in a previewed batch.
pub const MAX_ITEMS: usize = 20;
/// Maximum combined serialized journal data.
pub const MAX_BYTES: usize = 32 << 20;
/// A canonical application-generated UUID (also safe as an exact operation tag suffix).
pub fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn text_ok(s: &str, n: usize, multiline: bool) -> bool {
    s.len() <= n
        && !s
            .chars()
            .any(|c| c.is_control() && (!multiline || !matches!(c, '\n' | '\r' | '\t')))
}
/// Durable item outcome. Sending and uncertain failures must never be automatically resent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Previewed and journalled, never dispatched.
    Planned,
    /// Durable marker saved before handing work to the exporter.
    Sending,
    /// Confirmed note id, including a positively reconciled earlier commit.
    Created(u64),
    /// Explicit pending-card destination, not a live note.
    Kept,
    /// Existing word/card skipped before export.
    Duplicate,
    /// Certain or uncertain failure; uncertainty never permits a resend.
    Failed {
        /// Translated bounded reason.
        message: String,
        /// The exporter may already have committed.
        uncertain: bool,
    },
    /// Never dispatched because the remaining queue was cancelled.
    Cancelled,
}
impl Status {
    /// A commit may exist; only read-only reconciliation is permitted.
    pub fn unresolved(&self) -> bool {
        matches!(
            self,
            Self::Sending
                | Self::Failed {
                    uncertain: true,
                    ..
                }
        )
    }
    fn valid(&self) -> bool {
        match self {
            Self::Created(id) => *id > 0,
            Self::Failed { message, .. } => text_ok(message, 4096, true),
            _ => true,
        }
    }
    fn follows(&self, previous: &Self) -> bool {
        match previous {
            Self::Planned
            | Self::Cancelled
            | Self::Failed {
                uncertain: false, ..
            } => matches!(self, Self::Sending | Self::Cancelled),
            Self::Sending => matches!(
                self,
                Self::Created(_) | Self::Kept | Self::Duplicate | Self::Failed { .. }
            ),
            Self::Failed {
                uncertain: true, ..
            } => matches!(self, Self::Created(_)),
            _ => false,
        }
    }
}
/// Non-secret audit metadata; no API credentials, arbitrary paths or raw field mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Stable operation UUID, reused on certain retries and reconciliation.
    pub operation: String,
    /// Exact dictionary expression.
    pub expression: String,
    /// Exact dictionary reading.
    pub reading: String,
    /// Frozen original context.
    pub sentence: String,
    /// Original library source identity, absent for manual sources.
    pub source_uid: Option<String>,
    /// The original source is a book rather than a video link.
    pub source_book: bool,
    /// Original effective clip start.
    pub start_ms: u32,
    /// Original effective clip end.
    pub end_ms: u32,
    /// Source title.
    pub title: String,
    /// Configured destination kind.
    pub destination: String,
    /// Destination deck at preview time.
    pub deck: String,
    /// Destination note type at preview time.
    pub note_type: String,
    /// Durable outcome.
    pub status: Status,
}
impl Item {
    fn valid(&self) -> bool {
        valid_id(&self.operation)
            && !self.expression.is_empty()
            && text_ok(&self.expression, 4096, false)
            && text_ok(&self.reading, 4096, false)
            && text_ok(&self.sentence, 16384, true)
            && text_ok(&self.title, 1024, false)
            && !self.destination.is_empty()
            && text_ok(&self.destination, 32, false)
            && text_ok(&self.deck, 256, false)
            && text_ok(&self.note_type, 256, false)
            && self
                .source_uid
                .as_ref()
                .is_none_or(|s| !s.is_empty() && text_ok(s, 1024, false))
            && self.start_ms <= self.end_ms
            && self.end_ms <= i32::MAX as u32
            && self.status.valid()
    }
}
/// One explicitly started batch's operation identities and outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    /// Stable batch UUID.
    pub id: String,
    /// Creation time, Unix ms.
    pub created_ms: i64,
    /// At most twenty exact-identity items.
    pub items: Vec<Item>,
}
impl Journal {
    /// Validate all retained metadata before opening a write transaction.
    pub fn valid(&self) -> bool {
        let mut operations = std::collections::HashSet::new();
        let mut identities = std::collections::HashSet::new();
        valid_id(&self.id)
            && self.created_ms >= 0
            && !self.items.is_empty()
            && self.items.len() <= MAX_ITEMS
            && self.items.iter().all(|i| {
                i.valid()
                    && operations.insert(&i.operation)
                    && identities.insert((&i.expression, &i.reading))
            })
    }
    /// Whether a sending or uncertain item still requires reconciliation.
    pub fn unresolved(&self) -> bool {
        self.items.iter().any(|i| i.status.unresolved())
    }
}
impl Library {
    /// Complete bounded journals, newest first; absence is an empty history.
    pub fn batch_journals(&self) -> Result<Vec<Journal>, LibraryError> {
        self.read(|tx| {
            let t = match tx.open_table(JOURNALS) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
                Err(e) => return Err(LibraryError::Db(e.into())),
            };
            let mut rows = Vec::new();
            let mut bytes = 0usize;
            for row in db_err!(t.iter())? {
                let (_, value) = db_err!(row)?;
                bytes = bytes.saturating_add(value.value().len());
                if rows.len() >= MAX_BATCHES || bytes > MAX_BYTES {
                    return Err(LibraryError::Record(postcard::Error::SerdeDeCustom));
                }
                let row: Journal = postcard::from_bytes(value.value())?;
                if !row.valid() {
                    return Err(LibraryError::Record(postcard::Error::SerdeDeCustom));
                }
                rows.push(row);
            }
            rows.sort_by(|a, b| (b.created_ms, &b.id).cmp(&(a.created_ms, &a.id)));
            Ok(rows)
        })
    }
    /// Durably record a whole previewed selection before the first dispatch. Never overwrite an existing operation identity.
    pub fn start_batch(&self, journal: &Journal) -> Result<bool, LibraryError> {
        if !journal.valid() || journal.items.iter().any(|i| i.status != Status::Planned) {
            return Ok(false);
        }
        let data = postcard::to_stdvec(journal)?;
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(JOURNALS))?;
            if db_err!(t.get(journal.id.as_str()))?.is_some() {
                return Ok(false);
            }
            // Reserve each item's largest possible failure/status growth before any mutation.
            let mut bytes = data.len().saturating_add(journal.items.len() * 4112);
            let mut count = 0;
            let operations = journal
                .items
                .iter()
                .map(|i| i.operation.as_str())
                .collect::<std::collections::HashSet<_>>();
            for row in db_err!(t.iter())? {
                let (_, value) = db_err!(row)?;
                let prior: Journal = postcard::from_bytes(value.value())?;
                bytes = bytes
                    .saturating_add(value.value().len())
                    .saturating_add(prior.items.len() * 4112);
                count += 1;
                if prior
                    .items
                    .iter()
                    .any(|i| operations.contains(i.operation.as_str()))
                {
                    return Ok(false);
                }
                if prior.items.iter().any(|old| {
                    old.status.unresolved()
                        && journal.items.iter().any(|new| {
                            new.expression == old.expression
                                && new.reading == old.reading
                                && new.destination == old.destination
                                && new.deck == old.deck
                                && new.note_type == old.note_type
                        })
                }) {
                    return Ok(false);
                }
            }
            if bytes > MAX_BYTES || count >= MAX_BATCHES {
                return Ok(false);
            }
            db_err!(t.insert(journal.id.as_str(), data.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(true)
    }
    /// Compare-and-set one allowed outcome transition; an uncertain commit cannot be changed into a resend.
    pub fn change_batch_item(
        &self,
        batch: &str,
        operation: &str,
        expected: &Status,
        next: &Status,
    ) -> Result<bool, LibraryError> {
        if !next.valid() || !next.follows(expected) {
            return Ok(false);
        }
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(JOURNALS))?;
            let row = db_err!(t.get(batch))?
                .map(|v| postcard::from_bytes::<Journal>(v.value()))
                .transpose()?;
            let Some(mut row) = row else {
                return Ok(false);
            };
            let Some(item) = row
                .items
                .iter_mut()
                .find(|i| i.operation == operation && i.status == *expected)
            else {
                return Ok(false);
            };
            item.status = next.clone();
            db_err!(t.insert(batch, postcard::to_stdvec(&row)?.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(true)
    }
    /// Explicit cleanup is refused while any outcome is unresolved.
    pub fn delete_batch_journal(&self, id: &str) -> Result<bool, LibraryError> {
        let tx = self.write()?;
        let changed = {
            let mut t = db_err!(tx.open_table(JOURNALS))?;
            let row = db_err!(t.get(id))?
                .map(|v| postcard::from_bytes::<Journal>(v.value()))
                .transpose()?;
            if row.is_some_and(|r| r.unresolved()) {
                return Ok(false);
            }
            db_err!(t.remove(id))?.is_some()
        };
        db_err!(tx.commit())?;
        Ok(changed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn journal() -> Journal {
        Journal {
            id: "00000000-0000-4000-8000-000000000001".into(),
            created_ms: 1,
            items: vec![Item {
                operation: "00000000-0000-4000-8000-000000000002".into(),
                expression: "猫".into(),
                reading: "ねこ".into(),
                sentence: "猫です。".into(),
                source_uid: None,
                source_book: false,
                start_ms: 0,
                end_ms: 0,
                title: "Owned".into(),
                destination: "anki".into(),
                deck: "Lapis".into(),
                note_type: "Lapis".into(),
                status: Status::Planned,
            }],
        }
    }
    // T-BMG-004: write-ahead identity, compare-and-set outcomes, restart and no unsafe resend/deletion of an unknown commit.
    #[test]
    fn uncertain_batch_outcomes_survive_restart_and_cannot_be_resent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("l.redb");
        let lib = Library::open(&path).unwrap();
        let row = journal();
        let op = &row.items[0].operation;
        assert!(lib.start_batch(&row).unwrap());
        assert!(!lib.start_batch(&row).unwrap());
        assert!(
            lib.change_batch_item(&row.id, op, &Status::Planned, &Status::Sending)
                .unwrap()
        );
        assert!(
            !lib.change_batch_item(&row.id, op, &Status::Planned, &Status::Sending)
                .unwrap()
        );
        assert!(!lib.delete_batch_journal(&row.id).unwrap());
        let mut fresh = row.clone();
        fresh.id = "00000000-0000-4000-8000-000000000003".into();
        fresh.items[0].operation = "00000000-0000-4000-8000-000000000004".into();
        assert!(
            !lib.start_batch(&fresh).unwrap(),
            "a fresh batch cannot bypass unresolved same-destination sends"
        );
        drop(lib);
        let lib = Library::open(&path).unwrap();
        assert_eq!(
            lib.batch_journals().unwrap()[0].items[0].status,
            Status::Sending
        );
        let unknown = Status::Failed {
            message: "Commit unknown".into(),
            uncertain: true,
        };
        assert!(
            lib.change_batch_item(&row.id, op, &Status::Sending, &unknown)
                .unwrap()
        );
        assert!(
            !lib.change_batch_item(&row.id, op, &unknown, &Status::Sending)
                .unwrap()
        );
        assert!(
            lib.change_batch_item(&row.id, op, &unknown, &Status::Created(42))
                .unwrap()
        );
        assert!(
            !lib.change_batch_item(&row.id, op, &unknown, &Status::Created(43))
                .unwrap()
        );
        assert!(lib.delete_batch_journal(&row.id).unwrap());
        assert!(lib.word_statuses().unwrap().is_empty());
    }
}
