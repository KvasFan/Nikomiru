// SPDX-License-Identifier: GPL-3.0-only

//! Local sentence-bank jobs and verified original reader locations.
use super::*;
use crate::sentence_bank::{Answer, Data, Input, Job, Kind, Request};
use nikomiru_library::{
    phrases::Source as PhraseSource,
    sentences::{Batch, Example},
};
impl Session {
    /// Open source metadata without issuing any provider request.
    pub fn bank_open(&mut self) {
        self.bank.open = true;
        self.bank.jobs.push(Job::List);
    }
    /// Freeze the exact identity of an explicitly chosen dictionary entry.
    pub fn bank_examples(&mut self, expression: String, reading: String) {
        self.bank_open();
        self.bank_query(expression, reading);
    }
    /// Start a newer exact query; success and error replies carry the same epoch.
    pub fn bank_query(&mut self, expression: String, reading: String) {
        self.bank.epoch = self.bank.epoch.wrapping_add(1);
        self.bank.expression = expression.clone();
        self.bank.reading = reading.clone();
        self.bank.querying = true;
        self.bank.error = None;
        self.bank.jobs.push(Job::Query {
            epoch: self.bank.epoch,
            expression,
            reading,
            language: self.config().general.study_language.clone(),
        });
    }
    /// Index a frozen owned reader or complete primary text source, never an implicit filesystem scan.
    pub fn bank_index(&mut self, book: bool) {
        if self.bank.indexing || !self.phrases_writable() {
            return;
        }
        let language = self.config().general.study_language.clone();
        let request = if book {
            let Some(open) = &self.books_page.open else {
                self.bank.error = Some(self.tr("bank-error", &[]));
                return;
            };
            let paragraphs = open
                .book
                .chapters
                .iter()
                .enumerate()
                .flat_map(|(chapter, c)| {
                    c.paragraphs
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.image.is_none() && !p.text.trim().is_empty())
                        .map(move |(paragraph, p)| (chapter, paragraph, p))
                })
                .collect::<Vec<_>>();
            if open.book.manga
                || paragraphs.len() > 10000
                || paragraphs
                    .iter()
                    .map(|(_, _, p)| p.text.len())
                    .sum::<usize>()
                    > 8 << 20
            {
                self.bank.error = Some(self.tr("bank-error", &[]));
                return;
            }
            let inputs = paragraphs
                .into_iter()
                .map(|(chapter, paragraph, p)| Input {
                    text: p.text.clone(),
                    source: Some(PhraseSource {
                        book: true,
                        uid: open.uid.clone(),
                        title: open.book.title.chars().take(256).collect(),
                        chapter: chapter as u32,
                        paragraph: paragraph as u32,
                        offset: 0,
                        start_ms: 0,
                        end_ms: 0,
                    }),
                })
                .collect();
            Request::Captured {
                uid: open.uid.clone(),
                title: open.book.title.chars().take(256).collect(),
                language,
                split: true,
                inputs,
            }
        } else {
            let Some(uid) = self.current.as_ref().and_then(|c| c.uid.clone()) else {
                self.bank.error = Some(self.tr("bank-error", &[]));
                return;
            };
            let t = self.subs.timeline();
            if self.subs.incomplete
                || t.cues().is_empty()
                || t.cues().len() > 10000
                || t.cues().iter().map(|c| c.plain_text.len()).sum::<usize>() > 8 << 20
            {
                self.bank.error = Some(self.tr("bank-error", &[]));
                return;
            }
            let title = self.view.title.chars().take(256).collect::<String>();
            let inputs = t
                .cues()
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.plain_text.trim().is_empty())
                .map(|(paragraph, c)| {
                    let start = c
                        .start_ms
                        .saturating_add(t.delay_ms)
                        .clamp(0, i32::MAX as i64) as u32;
                    let end = c
                        .end_ms
                        .saturating_add(t.delay_ms)
                        .clamp(start as i64, i32::MAX as i64) as u32;
                    Input {
                        text: c.plain_text.clone(),
                        source: Some(PhraseSource {
                            book: false,
                            uid: uid.clone(),
                            title: title.clone(),
                            chapter: 0,
                            paragraph: paragraph as u32,
                            offset: 0,
                            start_ms: start,
                            end_ms: end,
                        }),
                    }
                })
                .collect();
            Request::Captured {
                uid,
                title,
                language,
                split: false,
                inputs,
            }
        };
        self.bank_prepare(request, book);
    }
    /// Explicitly chosen owned pack; all file reading/validation is on the bulk worker.
    pub fn bank_import(&mut self, path: PathBuf) {
        self.bank_prepare(
            Request::Import {
                path,
                language: self.config().general.study_language.clone(),
            },
            false,
        );
    }
    fn bank_prepare(&mut self, request: Request, book: bool) {
        if self.bank.indexing {
            return;
        }
        if !self.phrases_writable() {
            self.bank.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        let cancel = nikomiru_core::Cancel::new();
        self.bank.indexing = true;
        self.bank.book_index = book;
        self.bank.cancel = Some(cancel.clone());
        self.bank.prepare = Some((request, cancel));
        self.bank.error = None;
    }
    /// Dispatch requests independently of interactive lookups.
    pub fn take_bank_prepare(&mut self) -> Option<(Request, nikomiru_core::Cancel)> {
        self.bank.prepare.take()
    }
    /// Start real dictionary segmentation on the application's bulk worker.
    pub fn start_bank_prepare(
        &self,
        request: Request,
        cancel: nikomiru_core::Cancel,
        done: crate::dictionaries::Done<Batch>,
    ) {
        self.services
            .dictionaries
            .sentence_bank(request, cancel, done);
    }
    /// Commit only complete uncancelled snapshots, through the serial library writer.
    pub fn bank_prepared(&mut self, result: Result<Batch, String>) {
        let cancelled = self.bank.cancel.take().is_some_and(|c| c.is_cancelled());
        match result {
            Ok(batch) if !cancelled => self.bank.jobs.push(Job::Save(Box::new(batch))),
            result => {
                self.bank.indexing = false;
                self.bank.error = Some(if cancelled {
                    self.tr("bank-cancelled", &[])
                } else {
                    result.err().unwrap_or_else(|| self.tr("bank-error", &[]))
                });
            }
        }
    }
    /// Cancel bulk preparation on request or replacement of its active source; an atomic commit has no cancel button.
    pub fn bank_cancel(&mut self) {
        if let Some(cancel) = &self.bank.cancel {
            cancel.cancel();
        }
    }
    pub(super) fn bank_source_changed(&mut self, book: bool) {
        if self.bank.book_index == book {
            self.bank_cancel();
        }
    }
    /// Remove one retained batch explicitly.
    pub fn bank_delete(&mut self, key: String) {
        if !self.phrases_writable() {
            self.bank.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        self.bank.jobs.push(Job::Delete(key));
    }
    /// Take ordered reads/writes for dispatch.
    pub fn take_bank_jobs(&mut self) -> Vec<Job> {
        std::mem::take(&mut self.bank.jobs)
    }
    /// Ordered asynchronous query/save/delete, with no mutation of vocabulary or review tables.
    pub fn start_bank_job(&self, job: Job, done: Box<dyn FnOnce(Answer) + Send>) {
        let kind = match &job {
            Job::List => Kind::List,
            Job::Query { epoch, .. } => Kind::Query(*epoch),
            Job::Save(_) => Kind::Saved,
            Job::Delete(_) => Kind::Deleted,
        };
        let error = self.tr("bank-error", &[]);
        self.library_task(
            move |lib| {
                match job {
                    Job::Query {
                        expression,
                        reading,
                        language,
                        ..
                    } => {
                        return lib
                            .sentence_examples(&expression, &reading, &language)
                            .map(Data::Examples)
                            .map_err(|e| e.to_string());
                    }
                    Job::Save(batch) => {
                        if !lib.put_sentence_batch(&batch).map_err(|e| e.to_string())? {
                            return Err(error);
                        }
                    }
                    Job::Delete(key) => {
                        lib.delete_sentence_batch(&key).map_err(|e| e.to_string())?;
                    }
                    Job::List => {}
                }
                lib.sentence_batches()
                    .map(Data::Batches)
                    .map_err(|e| e.to_string())
            },
            Box::new(move |result| done(Answer { kind, result })),
        );
    }
    /// Apply only current query replies; a changed search cannot be overwritten by an older failure either.
    pub fn bank_done(&mut self, answer: Answer) {
        if !self.bank.accept_reply(answer.kind) {
            return;
        }
        match answer.result {
            Ok(Data::Batches(batches)) => {
                self.bank.batches = batches;
                if matches!(answer.kind, Kind::Saved | Kind::Deleted) {
                    self.bank.examples.clear();
                    if !self.bank.expression.is_empty() {
                        self.bank_query(self.bank.expression.clone(), self.bank.reading.clone());
                    }
                }
            }
            Ok(Data::Examples(examples)) => self.bank.examples = examples,
            Err(error) => self.bank.error = Some(error),
        }
    }
    /// Explicitly navigate a saved example, verifying its original reader bytes on completion.
    pub fn bank_source(&mut self, example: Example) -> Option<bool> {
        let source = example.sentence.source?;
        if !nikomiru_library::sentences::source_ok(&source) {
            return None;
        }
        if source.book {
            let record = self
                .services
                .library
                .as_ref()
                .and_then(|l| l.book(&source.uid).ok().flatten());
            let Some(record) = record.filter(|r| Path::new(&r.path).is_file()) else {
                self.bank.error = Some(self.tr("bank-source-missing", &[]));
                return None;
            };
            self.bank_place = Some((source, example.sentence.text));
            self.book_add(PathBuf::from(record.path));
            Some(true)
        } else {
            let link = nikomiru_anki::link::SourceLink {
                uid: source.uid,
                episode: 0,
                start_ms: source.start_ms,
                end_ms: source.end_ms,
                source_id: uuid::Uuid::new_v4().to_string(),
            };
            self.open_link(&link.to_url());
            Some(false)
        }
    }
    /// Freeze the displayed offline results before the native save dialog.
    pub fn bank_export_prepare(&mut self) -> bool {
        match crate::sentence_bank::tsv(&self.bank.examples) {
            Ok(text) => {
                self.bank_export = Some(text);
                true
            }
            Err(key) => {
                self.bank.error = Some(self.tr(key, &[]));
                false
            }
        }
    }
    /// Atomically save the frozen shown example snapshot.
    pub fn bank_export_to(&mut self, path: &Path) {
        if let Some(text) = self.bank_export.take() {
            match nikomiru_core::fs::write_atomic(path, text.as_bytes(), 0o600) {
                Ok(()) => self.toasts.push(self.tr(
                    "analysis-export-saved",
                    &[("path", path.display().to_string())],
                )),
                Err(e) => self
                    .toasts
                    .push(self.tr("toast-words-export-failed", &[("reason", e.to_string())])),
            }
        }
    }
}
