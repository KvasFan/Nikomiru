// SPDX-License-Identifier: GPL-3.0-only

//! Explicit batch builder, sequential dispatch and durable outcome/recovery barriers.
use super::*;
use crate::batch_mining::{
    After, Candidate, ContextSeed, Failure, JournalJob, JournalReply, Outcome, Preview, ServiceJob,
    Stage,
};
use crate::mining::{CardAction, FrameChoice, MineRequest};
use nikomiru_library::batches::{Item, Journal, Status};
fn text_request(
    query: String,
    expression: String,
    reading: String,
    matched: String,
    sentence: String,
    title: String,
    offset: Option<usize>,
) -> MineRequest {
    MineRequest {
        composition: Default::default(),
        query,
        entry: 0,
        expression,
        reading,
        matched,
        sentence,
        offset,
        video: PathBuf::new(),
        audio_input: PathBuf::new(),
        audio_stream: None,
        clip: None,
        position_ms: 0,
        uid: None,
        title,
        action: CardAction::Add,
        frame: FrameChoice::None,
        picture: None,
        audio: false,
    }
}
impl Session {
    /// Open the current builder and bounded journal history without starting an export.
    pub fn batch_open(&mut self) {
        self.batch.open = true;
        self.batch.journal_jobs.push(JournalJob::Load);
    }
    /// Start a fresh explicit selection; completed/unresolved history is retained separately.
    pub fn batch_new(&mut self) {
        if self.batch.stage.busy() {
            return;
        }
        self.batch.selection = Default::default();
        self.batch.active = None;
        self.batch.run.clear();
        self.batch.cursor = 0;
        self.batch.stop_requested = false;
        self.batch.stage = Stage::Idle;
        self.batch.error = None;
    }
    fn batch_can_edit(&mut self) -> bool {
        if self.batch.stage.busy() || self.batch.active.is_some() {
            self.batch.error = Some(self.tr("batch-new-required", &[]));
            return false;
        }
        true
    }
    /// Capture the explicitly chosen lookup entry and original source; retained frames are positions, never a later live click.
    pub fn batch_add_lookup(&mut self, level: usize, entry: usize) {
        self.batch_open();
        if !self.batch_can_edit() {
            return;
        }
        let from_source = matches!(
            self.popups.stack().first().map(|p| &p.origin),
            Some(Origin::Book { .. } | Origin::Subtitle { .. } | Origin::Hooked { .. })
        );
        let request = if from_source {
            self.mine_request(level, entry, CardAction::Add)
                .map(|(_, r)| r)
        } else {
            self.popups.stack().get(level).and_then(|p| {
                p.reply
                    .as_ref()
                    .and_then(|reply| reply.content.entries.get(entry))
                    .map(|e| {
                        text_request(
                            p.query.clone(),
                            e.expression.clone(),
                            e.reading.clone(),
                            e.matched.clone(),
                            String::new(),
                            String::new(),
                            None,
                        )
                    })
            })
        };
        let Some(mut request) = request else {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
            return;
        };
        if request.frame == FrameChoice::Click {
            request.frame = FrameChoice::At(request.position_ms);
        }
        let source_uid = if matches!(
            self.popups.stack().first().map(|p| &p.origin),
            Some(Origin::Book { .. })
        ) {
            self.books_page.open.as_ref().map(|o| o.uid.clone())
        } else {
            request.uid.clone()
        };
        if !self.batch.selection.add(Candidate {
            request,
            source_uid,
        }) {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
        } else {
            self.batch.stage = Stage::Idle;
            self.batch.error = None;
        }
    }
    /// Choose an owned saved example explicitly; it remains text-only and never opens an unrelated media file.
    pub fn batch_add_example(
        &mut self,
        example: nikomiru_library::sentences::Example,
        expression: String,
        reading: String,
    ) {
        self.batch_open();
        if !self.batch_can_edit() {
            return;
        }
        let Some(&(start, end)) = example.ranges.first() else {
            return;
        };
        let Some(surface) = example.sentence.text.get(start as usize..end as usize) else {
            return;
        };
        let source = example.sentence.source.as_ref();
        let title = source.map_or_else(|| example.title.clone(), |s| s.title.clone());
        let mut request = text_request(
            nikomiru_text::scan::normalize(&example.sentence.text[start as usize..]),
            expression,
            reading,
            surface.into(),
            example.sentence.text.clone(),
            title,
            Some(start as usize),
        );
        if let Some(s) = source.filter(|s| !s.book) {
            request.uid = Some(s.uid.clone());
            request.clip = Some((s.start_ms as i64, s.end_ms as i64));
            request.position_ms = s.start_ms as i64;
        }
        if !self.batch.selection.add(Candidate {
            request,
            source_uid: source.map(|s| s.uid.clone()),
        }) {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
        } else {
            self.batch.stage = Stage::Idle;
            self.batch.error = None;
        }
    }
    /// Prepare exact original primary-track contexts on the existing bulk dictionary worker.
    pub fn batch_add_words(&mut self, version: u64, rows: &[crate::words::ListedWord]) {
        self.batch_open();
        if !self.batch_can_edit() {
            return;
        }
        if version != self.sub_list_version
            || rows.is_empty()
            || rows.len() > 20
            || self.subs.incomplete
        {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
            return;
        }
        let Some(current) = &self.current else {
            return;
        };
        let Some(uid) = current.uid.clone() else {
            return;
        };
        let t = self.subs.timeline();
        if t.cues().is_empty()
            || t.cues().len() > 10000
            || t.cues().iter().map(|c| c.plain_text.len()).sum::<usize>() > 8 << 20
        {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
            return;
        }
        let title = self
            .view
            .title
            .chars()
            .filter(|c| !c.is_control())
            .take(256)
            .collect::<String>();
        let (audio_input, audio_stream) = self.audio_source(&current.path);
        let mut owner = text_request(
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            title.clone(),
            None,
        );
        owner.video = current.path.clone();
        owner.audio_input = audio_input;
        owner.audio_stream = audio_stream;
        owner.uid = Some(uid.clone());
        owner.audio = true;
        owner.frame = if self.audio_only() {
            FrameChoice::None
        } else {
            FrameChoice::At(0)
        };
        let inputs = t
            .cues()
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.plain_text.trim().is_empty())
            .map(|(paragraph, c)| crate::sentence_bank::Input {
                text: c.plain_text.clone(),
                source: Some(nikomiru_library::phrases::Source {
                    book: false,
                    uid: uid.clone(),
                    title: title.clone(),
                    chapter: 0,
                    paragraph: paragraph as u32,
                    offset: 0,
                    start_ms: c
                        .start_ms
                        .saturating_add(t.delay_ms)
                        .clamp(0, i32::MAX as i64) as u32,
                    end_ms: c
                        .end_ms
                        .saturating_add(t.delay_ms)
                        .clamp(0, i32::MAX as i64) as u32,
                }),
            })
            .collect();
        let request = crate::sentence_bank::Request::Captured {
            uid,
            title,
            language: self.config().general.study_language.clone(),
            split: false,
            inputs,
        };
        self.batch.seed = Some(ContextSeed {
            request: owner,
            wanted: rows
                .iter()
                .map(|r| (r.expression.clone(), r.reading.clone()))
                .collect(),
            clips: (0..t.cues().len()).map(|i| self.clip_of(i, i)).collect(),
        });
        let cancel = nikomiru_core::Cancel::new();
        self.batch.cancel = Some(cancel.clone());
        self.batch.prepare = Some((request, cancel));
        self.batch.stage = Stage::Preparing;
        self.batch.stop_requested = false;
        self.batch.error = None;
    }
    /// Pending source preparation for the UI dispatcher.
    pub fn take_batch_prepare(
        &mut self,
    ) -> Option<(crate::sentence_bank::Request, nikomiru_core::Cancel)> {
        self.batch.prepare.take()
    }
    /// Apply a complete exact segmented source; no partial selection survives overflow, cancellation or missing identities.
    pub fn batch_contexts(&mut self, result: Result<nikomiru_library::sentences::Batch, String>) {
        let cancelled = self.batch.cancel.take().is_some_and(|c| c.is_cancelled());
        let seed = self.batch.seed.take();
        self.batch.stage = Stage::Idle;
        if cancelled || self.batch.stop_requested {
            self.batch.error = Some(self.tr("batch-cancelled", &[]));
            return;
        }
        let batch = match result {
            Ok(b) => b,
            Err(e) => {
                self.batch.error = Some(e);
                return;
            }
        };
        let Some(seed) = seed else {
            return;
        };
        let mut counts = std::collections::HashMap::<(String, String), usize>::new();
        let mut candidates = Vec::new();
        for sentence in batch.sentences {
            for token in &sentence.tokens {
                let identity = (token.expression.clone(), token.reading.clone());
                if !seed.wanted.contains(&identity)
                    || counts.get(&identity).copied().unwrap_or(0) >= 5
                {
                    continue;
                }
                let Some(surface) = sentence.text.get(token.start as usize..token.end as usize)
                else {
                    continue;
                };
                let source = sentence.source.as_ref();
                let mut request = seed.request.clone();
                request.expression = identity.0.clone();
                request.reading = identity.1.clone();
                request.query =
                    nikomiru_text::scan::normalize(&sentence.text[token.start as usize..]);
                request.matched = surface.into();
                request.sentence = sentence.text.clone();
                request.offset = Some(token.start as usize);
                request.clip =
                    source.and_then(|s| seed.clips.get(s.paragraph as usize).copied().flatten());
                request.position_ms = request.clip.map_or(0, |c| c.0);
                if request.frame != FrameChoice::None {
                    request.frame = FrameChoice::At(request.position_ms);
                }
                candidates.push(Candidate {
                    source_uid: request.uid.clone(),
                    request,
                });
                *counts.entry(identity).or_default() += 1;
            }
        }
        if seed.wanted.iter().any(|w| !counts.contains_key(w))
            || !self.batch.selection.add_many(candidates)
        {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
        }
    }
    /// Explicit media/selection/context edits always invalidate prior previews.
    pub fn batch_edit(
        &mut self,
        index: usize,
        context: Option<usize>,
        enabled: Option<bool>,
        audio: Option<bool>,
        image: Option<bool>,
    ) {
        if !self.batch_can_edit() {
            return;
        }
        if let Some(context) = context {
            self.batch.selection.choose(index, context);
        }
        if let Some(e) = self.batch.selection.entries.get_mut(index) {
            if let Some(v) = enabled {
                e.enabled = v;
            }
            if let Some(v) = audio {
                e.audio = v;
            }
            if let Some(v) = image {
                e.image = v;
            }
            e.preview = None;
        }
        self.batch.stage = Stage::Idle;
    }
    /// Read-only whole-selection preview; Start remains a separate action.
    pub fn batch_preview(&mut self) {
        if !self.batch_can_edit() {
            return;
        }
        let requests = self.batch.selection.requests();
        if requests.is_empty() {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
            return;
        }
        self.batch.stage = Stage::Previewing;
        self.batch.stop_requested = false;
        self.batch.error = None;
        self.batch.service.push(ServiceJob::Preview(requests));
    }
    /// Accept complete previews only, matched by immutable operation identity.
    pub fn batch_previewed(&mut self, result: Result<Vec<Preview>, String>) {
        self.batch.stage = Stage::Idle;
        if self.batch.stop_requested {
            return;
        }
        match result {
            Ok(previews) => {
                for e in self
                    .batch
                    .selection
                    .entries
                    .iter_mut()
                    .filter(|e| e.enabled)
                {
                    e.preview = previews
                        .iter()
                        .find(|p| p.ticket.operation == e.operation)
                        .cloned();
                }
                if self
                    .batch
                    .selection
                    .entries
                    .iter()
                    .filter(|e| e.enabled)
                    .any(|e| e.preview.is_none())
                {
                    self.batch.error = Some(self.tr("batch-invalid", &[]));
                } else {
                    self.batch.stage = Stage::Ready;
                }
            }
            Err(e) => self.batch.error = Some(e),
        }
    }
    /// Persist the complete operation identities before the first exporter request.
    pub fn batch_start(&mut self) {
        if self.batch.stage != Stage::Ready || self.batch.active.is_some() {
            return;
        }
        if !self.phrases_writable() {
            self.batch.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        let mut items = Vec::new();
        let mut indices = Vec::new();
        for (i, e) in self
            .batch
            .selection
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.enabled)
        {
            let Some(p) = &e.preview else {
                return;
            };
            let r = &p.ticket.request;
            let times = r.clip.unwrap_or((r.position_ms, r.position_ms));
            items.push(Item {
                operation: e.operation.clone(),
                expression: r.expression.clone(),
                reading: r.reading.clone(),
                sentence: r.sentence.clone(),
                source_uid: e.candidate().and_then(|c| c.source_uid.clone()),
                source_book: r.uid.is_none()
                    && e.candidate().is_some_and(|c| c.source_uid.is_some()),
                start_ms: times.0.max(0) as u32,
                end_ms: times.1.max(0) as u32,
                title: r.title.clone(),
                destination: p.destination.clone(),
                deck: p.deck.clone(),
                note_type: p.note_type.clone(),
                status: Status::Planned,
            });
            indices.push(i);
        }
        let journal = Journal {
            id: uuid::Uuid::new_v4().to_string(),
            created_ms: self.phrase_now(),
            items,
        };
        if !journal.valid() {
            self.batch.error = Some(self.tr("batch-invalid", &[]));
            return;
        }
        self.batch.active = Some(journal.id.clone());
        self.batch.run = indices;
        self.batch.cursor = 0;
        self.batch.stop_requested = false;
        self.batch.stage = Stage::Starting;
        self.batch.journal_jobs.push(JournalJob::Start(journal));
    }
    fn batch_next(&mut self) {
        if self.batch.stop_requested {
            self.batch_stop_remaining();
            return;
        }
        let Some(&index) = self.batch.run.get(self.batch.cursor) else {
            self.batch.stage = Stage::Done;
            return;
        };
        let Some(batch) = self.batch.active.clone() else {
            return;
        };
        let entry = &self.batch.selection.entries[index];
        if entry.preview.is_none() || entry.status.unresolved() {
            self.batch.stage = Stage::Stopped;
            self.batch.error = Some(self.tr("batch-unknown", &[]));
            return;
        }
        self.batch.stage = Stage::Marking(index);
        self.batch.journal_jobs.push(JournalJob::Change {
            batch,
            operation: entry.operation.clone(),
            expected: entry.status.clone(),
            next: Status::Sending,
            after: After::Marked(index),
        });
    }
    /// Actual exporter outcome; record it before allowing another item to start.
    pub fn batch_mined(&mut self, result: Result<Outcome, Failure>) {
        let Stage::Sending(index) = self.batch.stage else {
            return;
        };
        let Some(batch) = self.batch.active.clone() else {
            return;
        };
        let next = match result {
            Ok(Outcome::Duplicate) => Status::Duplicate,
            Ok(Outcome::Mined(mined)) => {
                let source = self.batch.selection.entries[index]
                    .preview
                    .as_ref()
                    .and_then(|p| p.ticket.request.uid.as_ref());
                if source.is_some() && source == self.current.as_ref().and_then(|c| c.uid.as_ref())
                {
                    self.study.card(self.moment());
                }
                self.words.add_mined(
                    self.batch.selection.entries[index].contexts[0]
                        .request
                        .expression
                        .clone(),
                );
                if mined.kept.is_none() {
                    self.cards_made = true;
                    let r = &self.batch.selection.entries[index].contexts[0].request;
                    self.last_card = Some(crate::mining::LastCard {
                        note_id: mined.note_id,
                        expression: r.expression.clone(),
                        reading: r.reading.clone(),
                    });
                    self.apply_last_card();
                    self.events.push(UiEvent::CardCreated(mined.note_id));
                    Status::Created(mined.note_id)
                } else {
                    Status::Kept
                }
            }
            Err(failure) => Status::Failed {
                message: failure.message,
                uncertain: failure.uncertain,
            },
        };
        self.batch.stage = Stage::Recording(index);
        self.batch.journal_jobs.push(JournalJob::Change {
            batch,
            operation: self.batch.selection.entries[index].operation.clone(),
            expected: Status::Sending,
            next,
            after: After::Recorded(index),
        });
    }
    /// Cancel remaining scheduling; an in-flight export still reports its actual outcome.
    pub fn batch_cancel(&mut self) {
        self.batch.stop_requested = true;
        if let Some(c) = &self.batch.cancel {
            c.cancel();
        }
        if !self.batch.stage.busy() {
            self.batch_stop_remaining();
        }
    }
    fn batch_stop_remaining(&mut self) {
        if let Some(batch) = self.batch.active.clone() {
            for &index in self.batch.run.iter().skip(self.batch.cursor) {
                let e = &self.batch.selection.entries[index];
                if e.status == Status::Planned {
                    self.batch.journal_jobs.push(JournalJob::Change {
                        batch: batch.clone(),
                        operation: e.operation.clone(),
                        expected: Status::Planned,
                        next: Status::Cancelled,
                        after: After::StatusOnly,
                    });
                }
            }
        }
        self.batch.stage = Stage::Stopped;
    }
    /// Explicit certain retry, preserving the same preview and operation identity.
    pub fn batch_retry(&mut self, index: usize) {
        if self.batch.stage.busy() || self.batch.active.is_none() {
            return;
        }
        if self.batch.selection.entries.get(index).is_none_or(|e| {
            !matches!(
                e.status,
                Status::Failed {
                    uncertain: false,
                    ..
                } | Status::Cancelled
            )
        }) {
            return;
        }
        self.batch.run = vec![index];
        self.batch.cursor = 0;
        self.batch.stop_requested = false;
        self.batch.error = None;
        self.batch_next();
    }
    /// Continue never-sent items only after an explicit action and after all unknown outcomes have been resolved.
    pub fn batch_continue(&mut self) {
        if self.batch.stage.busy() {
            return;
        }
        let Some(id) = &self.batch.active else {
            return;
        };
        let Some(j) = self.batch.journals.iter().find(|j| &j.id == id) else {
            return;
        };
        if j.unresolved() {
            self.batch.error = Some(self.tr("batch-unknown", &[]));
            return;
        }
        self.batch.run = self
            .batch
            .selection
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                j.items.iter().any(|i| i.operation == e.operation)
                    && matches!(e.status, Status::Planned | Status::Cancelled)
            })
            .map(|(i, _)| i)
            .collect();
        self.batch.cursor = 0;
        self.batch.stop_requested = false;
        self.batch_next();
    }
    /// Read-only exact-tag reconciliation for current or old unresolved journal items.
    pub fn batch_reconcile(&mut self, batch: String, operation: String) {
        if self.batch.stage.busy() {
            return;
        }
        let item = self
            .batch
            .journals
            .iter()
            .find(|j| j.id == batch)
            .and_then(|j| j.items.iter().find(|i| i.operation == operation))
            .cloned();
        if let Some(item) = item.filter(|i| i.status.unresolved()) {
            self.batch.stage = Stage::Reconciling;
            self.batch.service.push(ServiceJob::Reconcile {
                batch,
                item: Box::new(item),
            });
        }
    }
    /// Zero/ambiguous/unavailable matches remain unresolved; no add request is queued.
    pub fn batch_reconciled(
        &mut self,
        batch: String,
        item: Item,
        result: Result<Option<u64>, String>,
    ) {
        self.batch.stage = Stage::Stopped;
        match result {
            Ok(Some(note)) => self.batch.journal_jobs.push(JournalJob::Recover {
                batch,
                item,
                note,
                now_ms: self.phrase_now(),
            }),
            Ok(None) => self.batch.error = Some(self.tr("batch-unknown", &[])),
            Err(e) => self.batch.error = Some(e),
        }
    }
    /// Explicitly remove resolved/never-sent history; unresolved records are refused by the library.
    pub fn batch_delete_journal(&mut self, id: String) {
        if !self.batch.stage.busy() && self.phrases_writable() {
            self.batch.journal_jobs.push(JournalJob::Delete(id));
        }
    }
    /// Pending Anki service work, bounded by the current state-machine stage.
    pub fn take_batch_service(&mut self) -> Vec<ServiceJob> {
        std::mem::take(&mut self.batch.service)
    }
    /// Pending ordered durable work.
    pub fn take_batch_journals(&mut self) -> Vec<JournalJob> {
        std::mem::take(&mut self.batch.journal_jobs)
    }
    /// Run all journal operations on the existing single library writer.
    pub fn start_batch_journal(&self, job: JournalJob, done: Box<dyn FnOnce(JournalReply) + Send>) {
        let after = match &job {
            JournalJob::Load => After::Loaded,
            JournalJob::Start(_) => After::Started,
            JournalJob::Change { after, .. } => after.clone(),
            JournalJob::Recover { item, note, .. } => After::Recovered {
                operation: item.operation.clone(),
                note: *note,
            },
            JournalJob::Delete(_) => After::Deleted,
        };
        let error = self.tr("batch-journal-error", &[]);
        self.library_task(
            move |lib| {
                let changed = match job {
                    JournalJob::Load => true,
                    JournalJob::Start(j) => lib.start_batch(&j).map_err(|e| e.to_string())?,
                    JournalJob::Change {
                        batch,
                        operation,
                        expected,
                        next,
                        ..
                    } => lib
                        .change_batch_item(&batch, &operation, &expected, &next)
                        .map_err(|e| e.to_string())?,
                    JournalJob::Recover {
                        batch,
                        item,
                        note,
                        now_ms,
                    } => {
                        let record = nikomiru_library::MinedCard {
                            expression: item.expression.clone(),
                            reading: item.reading.clone(),
                            sentence: item.sentence.clone(),
                            video_uid: if item.source_book {
                                String::new()
                            } else {
                                item.source_uid.clone().unwrap_or_default()
                            },
                            start_ms: item.start_ms,
                            end_ms: item.end_ms,
                            source_id: item.operation.clone(),
                            created_at: now_ms,
                        };
                        lib.put_mined_card(note, &record)
                            .map_err(|e| e.to_string())?;
                        lib.change_batch_item(
                            &batch,
                            &item.operation,
                            &item.status,
                            &Status::Created(note),
                        )
                        .map_err(|e| e.to_string())?
                    }
                    JournalJob::Delete(id) => {
                        lib.delete_batch_journal(&id).map_err(|e| e.to_string())?
                    }
                };
                if !changed {
                    return Err(error);
                }
                lib.batch_journals().map_err(|e| e.to_string())
            },
            Box::new(move |result| done(JournalReply { after, result })),
        );
    }
    /// Durable barriers are the only transitions that authorize the next export request.
    pub fn batch_journal_done(&mut self, reply: JournalReply) {
        let rows = match reply.result {
            Ok(rows) => rows,
            Err(e) => {
                self.batch.error = Some(e);
                if !matches!(
                    reply.after,
                    After::Loaded | After::Deleted | After::StatusOnly
                ) {
                    self.batch.stage = Stage::Stopped;
                }
                if matches!(reply.after, After::Started) {
                    self.batch.active = None;
                    self.batch.stage = Stage::Ready;
                }
                if !matches!(reply.after, After::Loaded) {
                    self.batch.journal_jobs.push(JournalJob::Load);
                }
                return;
            }
        };
        self.batch.journals = rows;
        if let Some(id) = &self.batch.active
            && let Some(j) = self.batch.journals.iter().find(|j| &j.id == id)
        {
            for e in &mut self.batch.selection.entries {
                if let Some(i) = j.items.iter().find(|i| i.operation == e.operation) {
                    e.status = i.status.clone();
                }
            }
        }
        match reply.after {
            After::Started => self.batch_next(),
            After::Marked(index) => {
                if self.batch.stop_requested {
                    let Some(batch) = self.batch.active.clone() else {
                        return;
                    };
                    self.batch.stage = Stage::Recording(index);
                    self.batch.journal_jobs.push(JournalJob::Change {
                        batch,
                        operation: self.batch.selection.entries[index].operation.clone(),
                        expected: Status::Sending,
                        next: Status::Failed {
                            message: self.tr("batch-cancelled", &[]),
                            uncertain: false,
                        },
                        after: After::Recorded(index),
                    });
                } else if let Some(preview) = self.batch.selection.entries[index].preview.clone() {
                    self.batch.stage = Stage::Sending(index);
                    self.batch
                        .service
                        .push(ServiceJob::Mine(Box::new(preview.ticket)));
                }
            }
            After::Recorded(index) => {
                if matches!(
                    self.batch.selection.entries[index].status,
                    Status::Failed { .. }
                ) {
                    self.batch.stage = Stage::Stopped;
                    if self.batch.stop_requested {
                        self.batch_stop_remaining();
                    }
                } else {
                    self.batch.cursor += 1;
                    self.batch_next();
                }
            }
            After::Recovered { operation, note } => {
                if let Some(item) = self
                    .batch
                    .journals
                    .iter()
                    .flat_map(|j| &j.items)
                    .find(|i| i.operation == operation)
                {
                    self.words.add_mined(item.expression.clone());
                }
                self.cards_made = true;
                self.events.push(UiEvent::CardCreated(note));
            }
            After::Loaded | After::Deleted | After::StatusOnly => {}
        }
    }
}
