// SPDX-License-Identifier: GPL-3.0-only

//! Explicit cloze operations, using the session's one ordered library writer.
use super::*;
use crate::cloze::{Answer, Effect, Job, Mode, Round};
impl Session {
    /// Open the exercise page and refresh persisted snapshots.
    pub fn cloze_open(&mut self) {
        self.clozes.open = true;
        self.clozes.round = None;
        self.clozes.preview = None;
        self.cloze_request(Job::List);
    }
    /// Prepare an immutable preview from a committed phrase; creation remains explicit.
    pub fn cloze_preview(&mut self, id: &str) {
        let Some(phrase) = self.phrases.items.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        self.cloze_open();
        self.clozes.preview = Some(phrase);
    }
    /// Queue one operation, rejecting writes from a read-only process.
    pub fn cloze_request(&mut self, job: Job) {
        if self.clozes.busy {
            return;
        }
        self.clozes.error = None;
        if !matches!(job, Job::List) && !self.phrases_writable() {
            self.clozes.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        self.clozes.busy = true;
        self.clozes.job = Some(job);
    }
    /// Deferred work for the UI event loop.
    pub fn take_cloze_job(&mut self) -> Option<Job> {
        self.clozes.job.take()
    }
    /// Execute on the same serial writer used by phrase edits and all other library writes.
    pub fn start_cloze_job(&self, job: Job, done: Box<dyn FnOnce(Answer) + Send>) {
        let error = self.tr("cloze-error", &[]);
        self.library_task(
            move |lib| {
                use nikomiru_library::cloze::Change;
                let mut effect = Effect::Loaded;
                match job {
                    Job::List => {}
                    Job::Create(p) => {
                        if !lib.create_cloze(&p).map_err(|e| e.to_string())? {
                            return Err(error);
                        }
                        effect = Effect::Created;
                    }
                    Job::Enrol(row) => {
                        if lib
                            .change_cloze(&row.phrase.id, None, Change::Enrol)
                            .map_err(|e| e.to_string())?
                            .is_none()
                        {
                            return Err(error);
                        }
                    }
                    Job::Rate {
                        row,
                        grade,
                        now_ms,
                        offset_s,
                        settings,
                    } => {
                        let before = row.memory.ok_or_else(|| error.clone())?;
                        let updated = lib
                            .change_cloze(
                                &row.phrase.id,
                                Some(before),
                                Change::Rate {
                                    grade,
                                    now_ms,
                                    offset_s,
                                    settings,
                                },
                            )
                            .map_err(|e| e.to_string())?
                            .ok_or_else(|| error.clone())?;
                        let after = updated.memory.ok_or_else(|| error.clone())?;
                        effect = Effect::Rated {
                            id: row.phrase.id,
                            before,
                            after,
                        };
                    }
                    Job::Undo { id, after, before } => {
                        if lib
                            .change_cloze(&id, Some(after), Change::Undo(before))
                            .map_err(|e| e.to_string())?
                            .is_none()
                        {
                            return Err(error);
                        }
                        effect = Effect::Undone(id);
                    }
                    Job::Delete(id) => {
                        lib.delete_cloze(&id).map_err(|e| e.to_string())?;
                    }
                }
                lib.clozes()
                    .map(|rows| (rows, effect))
                    .map_err(|e| e.to_string())
            },
            done,
        );
    }
    /// Apply committed results once; hidden/cancelled rounds never advance from old replies.
    pub fn cloze_done(&mut self, result: Answer) {
        self.clozes.busy = false;
        match result {
            Err(e) => self.clozes.error = Some(e),
            Ok((rows, effect)) => {
                self.clozes.choice_languages = crate::cloze::choice_languages(&rows);
                self.clozes.items = rows;
                match effect {
                    Effect::Created => self.clozes.preview = None,
                    Effect::Rated { id, before, after } => {
                        if let Some(round) = &mut self.clozes.round
                            && round.scheduled
                            && round.current().is_some_and(|r| r.phrase.id == id)
                        {
                            round.advance(false, Some((before, after)));
                        }
                    }
                    Effect::Undone(id) => {
                        if let Some(round) = &mut self.clozes.round
                            && round
                                .undo
                                .as_ref()
                                .is_some_and(|u| round.rows[u.index].phrase.id == id)
                        {
                            round.restore();
                        }
                    }
                    Effect::Loaded => {}
                }
            }
        }
    }
    /// Due snapshots selected at the click using the existing local-day/FSRS settings.
    pub fn cloze_due(&self) -> Vec<nikomiru_library::cloze::Exercise> {
        nikomiru_library::cloze::due(
            &self.clozes.items,
            self.phrase_now(),
            self.services.clock.utc_offset(self.services.clock.now()),
            self.config().review.new_per_day,
        )
    }
    /// Start an explicit bounded finite or due round; schedules stay unchanged until rating.
    pub fn cloze_start(&mut self, ids: &[String], mode: Mode, scheduled: bool) {
        if self.clozes.busy {
            return;
        }
        let rows = if scheduled {
            if !self.phrases_writable() {
                self.clozes.error = Some(self.tr("phrases-readonly", &[]));
                return;
            }
            self.cloze_due()
        } else {
            self.clozes
                .items
                .iter()
                .filter(|r| ids.contains(&r.phrase.id))
                .take(crate::cloze::ROUND_LIMIT)
                .cloned()
                .collect()
        };
        self.clozes.round = Round::new(rows, &self.clozes.items, mode, scheduled);
        self.clozes.error = self
            .clozes
            .round
            .is_none()
            .then(|| self.tr("cloze-unavailable", &[]));
    }
    /// Advance only after feedback; scheduled mode persists the explicit rating first.
    pub fn cloze_next(&mut self, grade: nikomiru_library::review::Grade, skip: bool) {
        if self.clozes.busy {
            return;
        }
        let Some(round) = &mut self.clozes.round else {
            return;
        };
        if !skip && round.feedback.is_none() {
            return;
        }
        if skip || !round.scheduled {
            round.advance(skip, None);
            return;
        }
        let Some(row) = round.current().cloned() else {
            return;
        };
        self.cloze_request(Job::Rate {
            row: Box::new(row),
            grade,
            now_ms: self.phrase_now(),
            offset_s: self.services.clock.utc_offset(self.services.clock.now()),
            settings: crate::review::settings(&self.config().review),
        });
    }
    /// Undo one finite step or atomically restore the immediately previous schedule memory.
    pub fn cloze_undo(&mut self) {
        if self.clozes.busy {
            return;
        }
        let Some(round) = &mut self.clozes.round else {
            return;
        };
        let Some(undo) = round.undo.clone() else {
            return;
        };
        if let Some((before, after)) = undo.memory {
            let id = round.rows[undo.index].phrase.id.clone();
            self.cloze_request(Job::Undo { id, after, before });
        } else {
            round.restore();
        }
    }
}
