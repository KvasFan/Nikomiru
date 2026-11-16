// SPDX-License-Identifier: GPL-3.0-only

//! Reader annotations, the library text search and reading modes
//! (RDR-001…RDR-004), as a child of the session module.
use super::*;
use crate::reader_tools::{Job, Scope, Tab};
use nikomiru_library::annotations::{Annotation, Kind};

impl Session {
    /// Open the reader tools page on `tab`, refreshing the annotations.
    pub fn reader_tools_open(&mut self, tab: Tab) {
        self.reader.open = true;
        self.reader.tab = tab;
        self.reader.this_book = self.books_page.open.as_ref().is_some_and(|o| !o.book.manga);
        self.close_all_popups();
        self.annotations_request(Job::List);
    }

    /// Queue one ordered annotation operation.
    pub fn annotations_request(&mut self, job: Job) {
        if self.reader.busy {
            return;
        }
        self.reader.error = None;
        if !matches!(job, Job::List) && !self.phrases_writable() {
            self.reader.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        self.reader.busy = true;
        self.annotation_job = Some(job);
    }

    /// Take the requested operation for the event loop.
    pub fn take_annotation_job(&mut self) -> Option<Job> {
        self.annotation_job.take()
    }

    /// Run it through the serial writer, or a reader when read-only.
    pub fn start_annotation_job(
        &self,
        job: Job,
        done: Box<dyn FnOnce(crate::reader_tools::Answer) + Send>,
    ) {
        let tr = self.translator_clone();
        self.library_task(
            move |lib: &Library| {
                let saved = match job {
                    Job::List => false,
                    Job::Save(a) => {
                        if !lib.put_annotation(&a).map_err(|e| e.to_string())? {
                            return Err(tr.tr("annotations-error", &[]));
                        }
                        true
                    }
                    Job::Delete(id) => {
                        lib.remove_annotation(&id).map_err(|e| e.to_string())?;
                        false
                    }
                };
                lib.annotations()
                    .map(|a| (a, saved))
                    .map_err(|e| e.to_string())
            },
            done,
        );
    }

    /// Commit a worker's snapshot.
    pub fn annotations_done(&mut self, answer: crate::reader_tools::Answer) {
        self.reader.busy = false;
        match answer {
            Ok((items, saved)) => {
                self.reader.items = items;
                if saved {
                    self.reader.editor = None;
                    // a new one from the reader: back to the reader
                    if self.annotation_capture.take().is_some() && self.books_page.open.is_some() {
                        self.reader.open = false;
                    }
                }
            }
            Err(error) => self.reader.error = Some(error),
        }
    }

    /// A draft over the looked-up word, or the paragraph at the top for a
    /// bookmark, in the open text book; nothing is saved yet.
    pub fn annotation_new(&mut self, kind: Kind) {
        if self.reader.busy {
            return;
        }
        let Some(open) = self.books_page.open.as_ref().filter(|o| !o.book.manga) else {
            self.reader.error = Some(self.tr("annotations-no-book", &[]));
            return;
        };
        let looked_up = self
            .books_page
            .highlight
            .clone()
            .or_else(|| self.books_page.found.clone());
        let (paragraph, range) = match (kind, looked_up) {
            (Kind::Bookmark, _) | (_, None) => (open.paragraph, 0..0),
            (_, Some(place)) => place,
        };
        let scope = if kind == Kind::Bookmark {
            Scope::Sentence
        } else {
            Scope::Word
        };
        self.annotation_capture = Some((open.chapter, paragraph, range));
        let now = self.phrase_now();
        let draft = Annotation {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            book: open.uid.clone(),
            title: open.book.title.chars().take(256).collect(),
            chapter: open.chapter as u32,
            paragraph: paragraph as u32,
            start: 0,
            end: 0,
            quote: String::new(),
            name: String::new(),
            note: String::new(),
            created_ms: now,
            changed_ms: now,
        };
        self.reader.editor = Some(draft);
        self.annotation_scope(scope);
        if let Some(draft) = &mut self.reader.editor
            && kind == Kind::Bookmark
        {
            draft.name = draft.quote.chars().take(40).collect();
        }
        self.reader.open = true;
        self.reader.tab = Tab::Annotations;
        self.reader.error = None;
        self.close_all_popups();
    }

    /// Cover the word, its sentence or its paragraph from the captured place.
    pub fn annotation_scope(&mut self, scope: Scope) {
        let Some((chapter, paragraph, range)) = self.annotation_capture.clone() else {
            return;
        };
        let Some(text) = self
            .books_page
            .open
            .as_ref()
            .filter(|o| self.reader.editor.as_ref().is_some_and(|e| e.book == o.uid))
            .and_then(|o| o.book.chapters.get(chapter))
            .and_then(|c| c.paragraphs.get(paragraph))
            .filter(|p| p.image.is_none())
            .map(|p| p.text.clone())
        else {
            return;
        };
        let covered = crate::reader_tools::scoped(&text, range, scope);
        let covered =
            crate::reader_tools::clip(&text, covered, nikomiru_library::annotations::MAX_QUOTE);
        if let Some(draft) = &mut self.reader.editor {
            draft.start = covered.start as u32;
            draft.end = covered.end as u32;
            draft.quote = text[covered].to_string();
        }
    }

    /// Drop the draft and its captured place.
    pub fn reader_discard_draft(&mut self) {
        self.reader.editor = None;
        self.annotation_capture = None;
    }

    /// Freeze the shown rows for the export dialog.
    pub fn annotations_export_prepare(&mut self, ids: &[String]) -> bool {
        let rows: Vec<Annotation> = self
            .reader
            .items
            .iter()
            .filter(|a| ids.contains(&a.id))
            .cloned()
            .collect();
        match crate::reader_tools::tsv(&rows) {
            Ok(text) => {
                self.annotation_export = Some(text);
                true
            }
            Err(key) => {
                self.toasts.push(self.tr(key, &[]));
                false
            }
        }
    }

    /// Write the frozen export atomically.
    pub fn annotations_export_to(&mut self, path: &Path) {
        if let Some(text) = self.annotation_export.take() {
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

    /// The open book's current highlights and notes in its chapter shown:
    /// their paragraphs and bytes (RDR-002).
    pub fn chapter_annotations(&self) -> Vec<(usize, std::ops::Range<usize>)> {
        let Some(open) = self.books_page.open.as_ref().filter(|o| !o.book.manga) else {
            return Vec::new();
        };
        let Some(chapter) = open.book.chapters.get(open.chapter) else {
            return Vec::new();
        };
        self.reader
            .items
            .iter()
            .filter(|a| {
                a.book == open.uid && a.kind != Kind::Bookmark && a.chapter as usize == open.chapter
            })
            .filter(|a| {
                chapter
                    .paragraphs
                    .get(a.paragraph as usize)
                    .is_some_and(|p| a.current_in(&p.text))
            })
            .map(|a| (a.paragraph as usize, a.start as usize..a.end as usize))
            .collect()
    }

    /// Save the draft.
    pub fn annotation_save(&mut self) {
        if let Some(mut a) = self.reader.editor.clone() {
            a.changed_ms = self.phrase_now().max(a.created_ms);
            self.annotations_request(Job::Save(a));
        }
    }

    /// Whether an annotation still matches its open book's text; `None` if
    /// its book is not the one open.
    pub fn annotation_current(&self, a: &Annotation) -> Option<bool> {
        let open = self.books_page.open.as_ref().filter(|o| o.uid == a.book)?;
        Some(
            open.book
                .chapters
                .get(a.chapter as usize)
                .and_then(|c| c.paragraphs.get(a.paragraph as usize))
                .is_some_and(|p| a.current_in(&p.text)),
        )
    }

    /// Go to an annotation: in the open book when it is still there, else by
    /// reopening its book with the quoted text checked (SEN-003).
    pub fn annotation_go(&mut self, id: &str) -> bool {
        let Some(a) = self.reader.items.iter().find(|a| a.id == id).cloned() else {
            return false;
        };
        self.go_to_text(
            &a.book,
            &a.title,
            a.chapter,
            a.paragraph,
            a.start,
            a.quote.clone(),
        )
    }

    /// Open a library search result.
    pub fn search_go(&mut self, index: usize) -> bool {
        let Some(hit) = self
            .reader
            .search
            .outcome
            .as_ref()
            .and_then(|o| o.hits.get(index))
            .cloned()
        else {
            return false;
        };
        self.go_to_text(
            &hit.uid,
            &hit.title,
            hit.chapter,
            hit.paragraph,
            hit.start,
            hit.text,
        )
    }

    fn go_to_text(
        &mut self,
        uid: &str,
        title: &str,
        chapter: u32,
        paragraph: u32,
        offset: u32,
        text: String,
    ) -> bool {
        let range = offset as usize..offset as usize + text.len();
        if let Some(open) = &mut self.books_page.open
            && open.uid == uid
        {
            let here = open
                .book
                .chapters
                .get(chapter as usize)
                .and_then(|c| c.paragraphs.get(paragraph as usize))
                .and_then(|p| p.text.get(range.clone()));
            if here != Some(text.as_str()) {
                let error = self.tr("bank-source-changed", &[]);
                self.reader.error = Some(error.clone());
                self.toasts.push(error);
                return false;
            }
            self.close_all_popups();
            self.book_save();
            if let Some(open) = &mut self.books_page.open {
                open.chapter = chapter as usize;
                open.paragraph = paragraph as usize;
                open.sentence = sentence_at(open, range.start);
                open.jump = true;
            }
            self.books_page.found = Some((paragraph as usize, range));
            return true;
        }
        self.books_reload();
        let path = self
            .books_page
            .list
            .iter()
            .find(|(u, b)| u == uid && Path::new(&b.path).is_file())
            .map(|(_, b)| PathBuf::from(&b.path));
        let Some(path) = path else {
            let error = self.tr("bank-source-missing", &[]);
            self.reader.error = Some(error.clone());
            self.toasts.push(error);
            return false;
        };
        self.bank_place = Some((
            nikomiru_library::phrases::Source {
                book: true,
                uid: uid.into(),
                title: title.into(),
                chapter,
                paragraph,
                offset,
                start_ms: 0,
                end_ms: 0,
            },
            text,
        ));
        self.book_add(path);
        true
    }

    /// Search the text of every book (RDR-003); older answers are dropped.
    pub fn search_run(&mut self, query: &str) {
        self.search_cancel();
        self.reader.search.query = query.into();
        self.reader.search.error = None;
        if nikomiru_text::literal::Needle::new(query).is_none() {
            self.reader.search.error = Some(self.tr("search-invalid", &[]));
            return;
        }
        self.books_reload();
        let books = self
            .books_page
            .list
            .iter()
            .map(|(uid, b)| (uid.clone(), PathBuf::from(&b.path), b.title.clone()))
            .collect();
        let cancel = nikomiru_core::Cancel::new();
        self.reader.search.cancel = Some(cancel.clone());
        self.reader.search.running = true;
        self.search_request = Some((
            crate::reader_tools::Request {
                epoch: self.reader.search.epoch,
                query: query.into(),
                books,
            },
            cancel,
        ));
    }

    /// Stop the search running; its answer will be dropped.
    pub fn search_cancel(&mut self) {
        if let Some(cancel) = self.reader.search.cancel.take() {
            cancel.cancel();
        }
        self.reader.search.epoch += 1;
        self.reader.search.running = false;
        self.search_request = None;
    }

    /// The search to start.
    pub fn take_search_request(
        &mut self,
    ) -> Option<(crate::reader_tools::Request, nikomiru_core::Cancel)> {
        self.search_request.take()
    }

    /// Run a search on a thread of its own, over the shared index.
    pub fn start_search(
        &self,
        request: crate::reader_tools::Request,
        cancel: nikomiru_core::Cancel,
        done: Box<dyn FnOnce(u64, Result<crate::reader_tools::Outcome, String>) + Send>,
    ) {
        let index = std::sync::Arc::clone(&self.search_index);
        let support = self.book_support();
        let epoch = request.epoch;
        let spawned = std::thread::Builder::new()
            .name("book-search".into())
            .spawn(move || {
                let result = crate::reader_tools::search(&request, &index, &*support, &cancel);
                done(epoch, result);
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the book search: {e}");
        }
    }

    /// A search's answer; a stale one changes nothing.
    pub fn search_done(
        &mut self,
        epoch: u64,
        result: Result<crate::reader_tools::Outcome, String>,
    ) {
        if epoch != self.reader.search.epoch || !self.reader.search.running {
            return;
        }
        self.reader.search.running = false;
        self.reader.search.cancel = None;
        match result {
            Ok(outcome) => self.reader.search.outcome = Some(outcome),
            Err(_) => self.reader.search.error = Some(self.tr("search-failed", &[])),
        }
    }

    /// The horizontal reading mode (RDR-004): `scroll`, `pages` or `sentence`.
    pub fn book_mode(&self) -> &str {
        match self.services.config.books.mode.as_str() {
            "pages" => "pages",
            "sentence" => "sentence",
            _ => "scroll",
        }
    }

    /// The mode whose pages or sentences Page Up/Down turn: the open text
    /// book's, when it is written horizontally and not scrolled.
    pub fn book_turning(&self) -> Option<&'static str> {
        let open = self.books_page.open.as_ref()?;
        if open.book.manga || self.book_vertical() {
            return None;
        }
        match self.book_mode() {
            "pages" => Some("pages"),
            "sentence" => Some("sentence"),
            _ => None,
        }
    }

    /// Choose the horizontal reading mode.
    pub fn book_set_mode(&mut self, mode: &str) {
        if !matches!(mode, "scroll" | "pages" | "sentence") {
            return;
        }
        self.close_all_popups();
        self.services.config.books.mode = mode.into();
        if let Some(open) = &mut self.books_page.open {
            open.jump = true;
            open.sentence = 0;
        }
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        }
    }

    /// An explicit page turn to the page starting at `paragraph`.
    pub fn book_turn(&mut self, paragraph: usize) {
        self.close_all_popups();
        if let Some(open) = &mut self.books_page.open {
            open.paragraph = paragraph;
            open.sentence = 0;
            open.jump = false;
        }
        self.book_save();
    }

    /// The next (`forward`) or previous sentence, across paragraphs and
    /// chapters, in sentence focus.
    pub fn book_sentence(&mut self, forward: bool) {
        self.close_all_popups();
        let Some(open) = &mut self.books_page.open else {
            return;
        };
        let count = |open: &crate::books::OpenBook, c: usize, p: usize| {
            open.book
                .chapters
                .get(c)
                .and_then(|c| c.paragraphs.get(p))
                .filter(|p| p.image.is_none())
                .map_or(0, |p| nikomiru_text::sentence::sentences(&p.text).len())
        };
        let (mut c, mut p, mut s) = (open.chapter, open.paragraph, open.sentence);
        let chapters = open.book.chapters.len();
        loop {
            if forward {
                if s + 1 < count(open, c, p) {
                    s += 1;
                    break;
                }
                s = 0;
                p += 1;
                if p >= open
                    .book
                    .chapters
                    .get(c)
                    .map_or(0, |ch| ch.paragraphs.len())
                {
                    if c + 1 >= chapters {
                        return;
                    }
                    c += 1;
                    p = 0;
                }
                if count(open, c, p) > 0 {
                    break;
                }
            } else {
                if s > 0 && s <= count(open, c, p) {
                    s -= 1;
                    break;
                }
                if p == 0 {
                    if c == 0 {
                        return;
                    }
                    c -= 1;
                    p = open
                        .book
                        .chapters
                        .get(c)
                        .map_or(0, |ch| ch.paragraphs.len());
                    if p == 0 {
                        continue;
                    }
                }
                p -= 1;
                let n = count(open, c, p);
                if n > 0 {
                    s = n - 1;
                    break;
                }
                s = 0;
            }
        }
        let changed_chapter = c != open.chapter;
        open.chapter = c;
        open.paragraph = p;
        open.sentence = s;
        open.jump = changed_chapter;
        self.books_page.found = None;
        self.book_save();
    }
}

/// The sentence of the open paragraph holding `offset`.
fn sentence_at(open: &crate::books::OpenBook, offset: usize) -> usize {
    open.book
        .chapters
        .get(open.chapter)
        .and_then(|c| c.paragraphs.get(open.paragraph))
        .map_or(0, |p| {
            nikomiru_text::sentence::sentences(&p.text)
                .iter()
                .position(|r| r.end > offset)
                .unwrap_or(0)
        })
}
