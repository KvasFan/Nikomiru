// SPDX-License-Identifier: GPL-3.0-only

//! Notebook operations and frozen sources, as a child of the session module.
use super::*;
impl Session {
    /// Whether this process owns the writable library.
    pub fn phrases_writable(&self) -> bool {
        self.writer.is_some()
    }
    /// Current Unix time for an explicit draft edit.
    pub fn phrase_now(&self) -> i64 {
        self.now_ms().max(0)
    }
    /// Queue a single ordered operation; the editor remains intact until success.
    pub fn phrases_request(&mut self, job: crate::phrases::Job) {
        if self.phrases.busy {
            return;
        }
        self.phrases.error = None;
        if !matches!(job, crate::phrases::Job::List) && !self.phrases_writable() {
            self.phrases.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        self.phrases.busy = true;
        self.phrase_job = Some(job);
    }
    /// Take explicitly requested work for the event loop.
    pub fn take_phrase_job(&mut self) -> Option<crate::phrases::Job> {
        self.phrase_job.take()
    }
    /// Dispatch through the existing serial writer; read-only snapshots use a worker.
    pub fn start_phrase_job(
        &self,
        job: crate::phrases::Job,
        done: Box<dyn FnOnce(crate::phrases::Answer) + Send>,
    ) {
        if self.services.library.is_none() {
            done(Err(self.tr("phrases-readonly", &[])));
            return;
        }
        let tr = self.translator_clone();
        let run = move |lib: &Library| {
            let result = (|| {
                let saved = match job {
                    crate::phrases::Job::List => false,
                    crate::phrases::Job::Save(p) => {
                        if !lib.put_phrase(&p).map_err(|e| e.to_string())? {
                            return Err(tr.tr("phrases-error", &[]));
                        }
                        true
                    }
                    crate::phrases::Job::Delete(id) => {
                        lib.remove_phrase(&id).map_err(|e| e.to_string())?;
                        false
                    }
                };
                lib.phrases().map(|p| (p, saved)).map_err(|e| e.to_string())
            })();
            done(result);
        };
        if let Some(writer) = &self.writer {
            writer.run(run);
        } else if let Some(lib) = self.services.library.clone() {
            // A channel makes even thread creation failure observable, without dropping the callback.
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("phrase-read".into())
                .spawn(move || {
                    if let Ok(run) = rx.recv() {
                        let run: Box<dyn FnOnce(&Library) + Send> = run;
                        run(&lib);
                    }
                }) {
                Ok(_) => {
                    let _ = tx.send(Box::new(run) as Box<dyn FnOnce(&Library) + Send>);
                }
                Err(e) => {
                    tracing::error!("phrase worker: {e}");
                    if let Some(lib) = &self.services.library {
                        run(lib);
                    }
                }
            }
        }
    }
    /// Commit a worker result to the visible snapshot.
    pub fn phrases_done(&mut self, answer: crate::phrases::Answer) {
        self.phrases.busy = false;
        match answer {
            Ok((items, saved)) => {
                self.phrases.items = items;
                if saved {
                    self.phrases.editor = None;
                }
            }
            Err(error) => self.phrases.error = Some(error),
        }
    }
    /// Freeze an explicitly captured source, or start a blank manual phrase.
    pub fn phrase_new(&mut self, capture: bool) {
        if self.phrases.busy {
            return;
        }
        let mut p = nikomiru_library::phrases::Phrase {
            id: uuid::Uuid::new_v4().to_string(),
            language: self.config().general.study_language.clone(),
            text: String::new(),
            definition: String::new(),
            context: String::new(),
            status: crate::words::WordStatus::Unknown,
            source: None,
            created_ms: self.phrase_now(),
            changed_ms: self.phrase_now(),
        };
        if capture {
            if let Some(open) = &self.books_page.open {
                let (paragraph, range) = self
                    .books_page
                    .highlight
                    .clone()
                    .unwrap_or((open.paragraph, 0..0));
                if let Some(para) = open
                    .book
                    .chapters
                    .get(open.chapter)
                    .and_then(|c| c.paragraphs.get(paragraph))
                {
                    let text = &para.text;
                    let range = if text.get(range.clone()).is_some() {
                        range
                    } else {
                        0..0
                    };
                    let sentence = nikomiru_text::sentence::sentence(text, range.clone());
                    if let Some(context) = text.get(sentence.range).filter(|s| s.len() <= 16384) {
                        p.context = context.into();
                        p.text = text.get(range.clone()).unwrap_or_default().into();
                        if p.text.is_empty()
                            && context.len() <= 4096
                            && !context.chars().any(char::is_control)
                        {
                            p.text = context.into();
                        }
                    }
                    p.source = Some(nikomiru_library::phrases::Source {
                        book: true,
                        uid: open.uid.clone(),
                        title: open.book.title.chars().take(256).collect(),
                        chapter: open.chapter as u32,
                        paragraph: paragraph as u32,
                        offset: range.start as u32,
                        start_ms: 0,
                        end_ms: 0,
                    });
                }
            } else if let Some(cue) = self
                .reference_cue()
                .and_then(|i| self.subs.timeline().cues().get(i))
            {
                let t = self.subs.timeline();
                if cue.plain_text.len() <= 16384 {
                    p.context.clone_from(&cue.plain_text);
                }
                if p.context.len() <= 4096 && !p.context.chars().any(char::is_control) {
                    p.text.clone_from(&p.context);
                }
                if let Some(uid) = self.current.as_ref().and_then(|c| c.uid.clone()) {
                    let start = (cue.start_ms + t.delay_ms).clamp(0, i32::MAX as i64) as u32;
                    let end = (cue.end_ms + t.delay_ms).clamp(start as i64, i32::MAX as i64) as u32;
                    p.source = Some(nikomiru_library::phrases::Source {
                        book: false,
                        uid,
                        title: self.view.title.chars().take(256).collect(),
                        chapter: 0,
                        paragraph: 0,
                        offset: 0,
                        start_ms: start,
                        end_ms: end,
                    });
                }
            }
        }
        self.phrases.editor = Some(p);
        self.phrases.error = None;
    }
    /// Explicitly reopen a retained source; a successful request indicates whether to show Books.
    pub fn phrase_source(&mut self, id: &str) -> Option<bool> {
        let source = self
            .phrases
            .items
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.source.clone())?;
        if source.book {
            self.books_reload();
            if !self
                .books_page
                .list
                .iter()
                .any(|(uid, b)| *uid == source.uid && Path::new(&b.path).is_file())
            {
                self.toasts.push(self.tr("phrases-source-missing", &[]));
                return None;
            }
            let uid = source.uid.clone();
            self.phrase_place = Some(source);
            self.book_choose(&uid);
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
    /// Freeze only currently displayed ids at the export click.
    pub fn phrases_export_prepare(&mut self, ids: &[String]) -> bool {
        let rows = self
            .phrases
            .items
            .iter()
            .filter(|p| ids.contains(&p.id))
            .cloned()
            .collect::<Vec<_>>();
        match crate::phrases::tsv(&rows) {
            Ok(text) => {
                self.phrase_export = Some(text);
                true
            }
            Err(key) => {
                self.toasts.push(self.tr(key, &[]));
                false
            }
        }
    }
    /// Write the captured export atomically.
    pub fn phrases_export_to(&mut self, path: &Path) {
        if let Some(text) = self.phrase_export.take() {
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
