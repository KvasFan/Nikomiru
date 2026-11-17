// SPDX-License-Identifier: GPL-3.0-only

//! Frozen phrase exercises and single-owner opt-in FSRS memory (CLO-001…CLO-004).
use crate::{
    Library, LibraryError,
    db::db_err,
    phrases::Phrase,
    review::{Grade, Memory, Settings},
};
use nikomiru_text::cloze::{answer, mask};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
const CLOZES: TableDefinition<&str, &[u8]> = TableDefinition::new("cloze_exercises");
/// Immutable exercise snapshot with optional opt-in FSRS memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exercise {
    /// Frozen phrase, meaning, context and source. The phrase id is this exercise's stable id.
    pub phrase: Phrase,
    /// None until explicitly enrolled; finite practice never writes this field.
    pub memory: Option<Memory>,
}
impl Exercise {
    /// Concealed prompt; it contains no definition or unmasked equivalent answer.
    pub fn prompt(&self) -> Option<String> {
        mask(&self.phrase.context, &self.phrase.text)
    }
    /// Literal surface answer comparison; alternate readings are not inferred.
    pub fn accepts(&self, input: &str) -> bool {
        answer(input) == answer(&self.phrase.text)
    }
    /// Validate snapshot bounds and canonical concealment.
    pub fn valid(&self) -> bool {
        self.phrase.valid() && self.prompt().is_some()
    }
}
/// Explicit schedule mutation, guarded by the caller's expected previous memory.
#[derive(Debug, Clone, Copy)]
pub enum Change {
    /// Enrol a previously unscheduled exercise.
    Enrol,
    /// A revealed exercise's explicit FSRS rating.
    Rate {
        /// The rating.
        grade: Grade,
        /// Unix ms.
        now_ms: i64,
        /// Local time offset.
        offset_s: i32,
        /// Existing review settings.
        settings: Settings,
    },
    /// Restore the immediately previous snapshot; callers keep only one undo opportunity.
    Undo(Memory),
}
impl Library {
    /// All bounded exercise snapshots, including future scheduled exercises.
    pub fn clozes(&self) -> Result<Vec<Exercise>, LibraryError> {
        self.read(|tx| {
            let t = match tx.open_table(CLOZES) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
                Err(e) => return Err(LibraryError::Db(e.into())),
            };
            let mut rows = Vec::new();
            let mut bytes = 0usize;
            for row in db_err!(t.iter())? {
                let (_, data) = db_err!(row)?;
                bytes = bytes.saturating_add(data.value().len());
                let row: Exercise = postcard::from_bytes(data.value())?;
                if rows.len() >= 10000 || bytes > 32 << 20 || !row.valid() {
                    return Err(LibraryError::Record(postcard::Error::SerdeDeCustom));
                }
                rows.push(row);
            }
            rows.sort_by(|a, b| {
                (a.phrase.created_ms, &a.phrase.id).cmp(&(b.phrase.created_ms, &b.phrase.id))
            });
            Ok(rows)
        })
    }
    /// Create one frozen exercise after preview; duplicate ids/canonical exercises never reset memory.
    pub fn create_cloze(&self, phrase: &Phrase) -> Result<bool, LibraryError> {
        let row = Exercise {
            phrase: phrase.clone(),
            memory: None,
        };
        if !row.valid() {
            return Ok(false);
        }
        let data = postcard::to_stdvec(&row)?;
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(CLOZES))?;
            let mut count = 0usize;
            let mut bytes = data.len();
            for entry in db_err!(t.iter())? {
                let (key, v) = db_err!(entry)?;
                let other: Exercise = postcard::from_bytes(v.value())?;
                if key.value() == phrase.id
                    || (other.phrase.language == phrase.language
                        && answer(&other.phrase.text) == answer(&phrase.text)
                        && answer(&other.phrase.context) == answer(&phrase.context))
                {
                    return Ok(false);
                }
                count += 1;
                bytes = bytes.saturating_add(v.value().len());
            }
            if count >= 10000 || bytes > 32 << 20 {
                return Ok(false);
            }
            db_err!(t.insert(phrase.id.as_str(), data.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(true)
    }
    /// Atomically mutate exactly one schedule, refusing stale/double ratings and undo.
    pub fn change_cloze(
        &self,
        id: &str,
        expected: Option<Memory>,
        change: Change,
    ) -> Result<Option<Exercise>, LibraryError> {
        let tx = self.write()?;
        let result = {
            let mut t = db_err!(tx.open_table(CLOZES))?;
            let row = db_err!(t.get(id))?
                .map(|v| postcard::from_bytes::<Exercise>(v.value()))
                .transpose()?;
            let Some(mut row) = row else {
                return Ok(None);
            };
            if row.memory != expected {
                return Ok(None);
            }
            row.memory = Some(match (change, row.memory) {
                (Change::Enrol, None) => Memory::default(),
                (
                    Change::Rate {
                        grade,
                        now_ms,
                        offset_s,
                        settings,
                    },
                    Some(memory),
                ) if now_ms > 0 && memory.reps < u32::MAX => {
                    crate::review::answer(&memory, grade, now_ms, offset_s, &settings)
                }
                (Change::Undo(prior), Some(_)) => prior,
                _ => return Ok(None),
            });
            db_err!(t.insert(id, postcard::to_stdvec(&row)?.as_slice()))?;
            row
        };
        db_err!(tx.commit())?;
        Ok(Some(result))
    }
    /// Explicitly delete an exercise and its schedule together; the original phrase is untouched.
    pub fn delete_cloze(&self, id: &str) -> Result<bool, LibraryError> {
        let tx = self.write()?;
        let removed = {
            let mut t = db_err!(tx.open_table(CLOZES))?;
            db_err!(t.remove(id))?.is_some()
        };
        db_err!(tx.commit())?;
        Ok(removed)
    }
}
/// Frozen due queue using existing rollover and daily-new settings; no scheduling side effects.
pub fn due(rows: &[Exercise], now_ms: i64, offset_s: i32, new_per_day: u32) -> Vec<Exercise> {
    use crate::review::{Phase, day_of};
    let today = day_of(now_ms, offset_s);
    let introduced = rows
        .iter()
        .filter_map(|r| r.memory)
        .filter(|m| m.first_ms > 0 && day_of(m.first_ms, offset_s) == today)
        .count();
    let mut new_left = (new_per_day as usize).saturating_sub(introduced);
    let mut queue = rows
        .iter()
        .filter_map(|r| r.memory.map(|m| (r, m)))
        .filter(|(_, m)| m.phase == Phase::New || m.due_ms <= now_ms)
        .collect::<Vec<_>>();
    queue.sort_by_key(|(r, m)| (m.phase == Phase::New, m.due_ms, r.phrase.created_ms));
    queue
        .into_iter()
        .filter_map(|(r, m)| {
            if m.phase == Phase::New {
                if new_left == 0 {
                    return None;
                }
                new_left -= 1;
            }
            Some(r.clone())
        })
        .take(100)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn phrase() -> Phrase {
        Phrase {
            id: "00000000-0000-4000-8000-000000000001".into(),
            language: "ja".into(),
            text: "猫".into(),
            definition: "cat".into(),
            context: "猫と猫です。".into(),
            status: crate::words::WordStatus::Unknown,
            source: None,
            created_ms: 1,
            changed_ms: 1,
        }
    }
    // T-CLO-001, T-CLO-004: real transactions, canonical deduplication, exact engine memory and one-shot stale-safe undo/restart.
    #[test]
    fn cloze_schedule_is_explicit_atomic_and_persistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("l.redb");
        let lib = Library::open(&path).unwrap();
        let p = phrase();
        assert!(lib.create_cloze(&p).unwrap());
        assert!(!lib.create_cloze(&p).unwrap());
        let initial = lib.clozes().unwrap();
        assert!(initial[0].memory.is_none());
        assert_eq!(initial[0].prompt().unwrap(), "[…]と[…]です。");
        assert!(!initial[0].accepts("ねこ"));
        let enrolled = lib
            .change_cloze(&p.id, None, Change::Enrol)
            .unwrap()
            .unwrap();
        assert!(!due(std::slice::from_ref(&enrolled), 1000, 0, 1).is_empty());
        let memory = enrolled.memory.unwrap();
        let settings = Settings {
            retention: 0.9,
            max_days: 36500,
            new_per_day: 20,
        };
        let change = Change::Rate {
            grade: Grade::Good,
            now_ms: 1000,
            offset_s: 0,
            settings,
        };
        let updated = lib
            .change_cloze(&p.id, Some(memory), change)
            .unwrap()
            .unwrap();
        assert_eq!(
            updated.memory,
            Some(crate::review::answer(
                &memory,
                Grade::Good,
                1000,
                0,
                &settings
            ))
        );
        assert!(
            lib.change_cloze(&p.id, Some(memory), change)
                .unwrap()
                .is_none()
        );
        assert!(due(std::slice::from_ref(&updated), 1000, 0, 1).is_empty());
        assert!(
            lib.change_cloze(&p.id, updated.memory, Change::Undo(memory))
                .unwrap()
                .is_some()
        );
        assert!(
            lib.change_cloze(&p.id, updated.memory, Change::Undo(memory))
                .unwrap()
                .is_none()
        );
        drop(lib);
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.clozes().unwrap(), vec![enrolled]);
        assert!(lib.word_statuses().unwrap().is_empty());
        assert!(lib.review_cards().unwrap().is_empty());
        assert!(lib.delete_cloze(&p.id).unwrap());
        assert!(lib.clozes().unwrap().is_empty());
    }
}
