// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary service of the session: lookups run on a worker thread,
//! with the dictionaries of the registry (DIC-021), and come back as popup
//! content (POP-007); imports run on their own thread; the page's other
//! operations are quick registry writes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use nikomiru_core::Cancel;
use nikomiru_dict::import::Progress;

use nikomiru_dict::css::Sheet;
use nikomiru_dict::lookup::Options;
use nikomiru_library::{
    Check, Collapse as RecordCollapse, DictionaryError, DictionaryManager, DictionaryRecord,
};
use nikomiru_ui::dictionaries::{DictionaryService, Done};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::popup::Reply;
use nikomiru_ui::popup_view::{self, Collapse, DictView};

/// The dictionaries of the registry: lookups on a worker thread, in order.
#[derive(Debug)]
pub struct Dictionaries {
    manager: Arc<DictionaryManager>,
    translator: Translator,
    jobs: Option<mpsc::Sender<(String, Done<Reply>)>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Dictionaries {
    /// The worker holds the library (through the manager): it must end
    /// before the process does, or the library is left to be repaired.
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Dictionaries {
    /// Starts the lookup worker.
    pub fn start(
        manager: Arc<DictionaryManager>,
        translator: Translator,
        max_results: usize,
    ) -> Self {
        let (jobs, rx) = mpsc::channel::<(String, Done<Reply>)>();
        let worker_manager = Arc::clone(&manager);
        let worker_translator = translator.clone();
        let spawned = std::thread::Builder::new()
            .name("nikomiru-lookup".into())
            .spawn(move || {
                for (text, done) in rx {
                    let started = std::time::Instant::now();
                    let result = render(&worker_manager, &text, max_results)
                        .map_err(|e| worker_translator.error(&e));
                    tracing::debug!(query = %text, elapsed = ?started.elapsed(), "lookup");
                    done(result);
                }
            });
        let worker = match spawned {
            Ok(worker) => Some(worker),
            Err(e) => {
                tracing::error!("cannot start the lookup thread: {e}");
                None
            }
        };
        Self {
            manager,
            translator,
            jobs: Some(jobs),
            worker,
        }
    }

    fn tr<T>(&self, r: Result<T, DictionaryError>) -> Result<T, String> {
        r.map_err(|e| self.translator.error(&e))
    }
}

impl DictionaryService for Dictionaries {
    fn lookup(&self, text: String, done: Done<Reply>) {
        let unsent = match &self.jobs {
            Some(jobs) => jobs
                .send((text, done))
                .err()
                .map(|mpsc::SendError((_, done))| done),
            None => Some(done),
        };
        if let Some(done) = unsent {
            done(Err(self.translator.tr("error-dict-unavailable", &[])));
        }
    }

    fn list(&self) -> Result<Vec<(String, DictionaryRecord)>, String> {
        self.tr(self.manager.list())
    }

    fn check(&self, zip: &Path) -> Result<Check, String> {
        self.tr(self.manager.check(zip))
    }

    fn import(
        &self,
        zip: PathBuf,
        replace: Option<String>,
        cancel: Cancel,
        progress: Box<dyn Fn(Progress) + Send>,
        done: Done<String>,
    ) {
        let manager = Arc::clone(&self.manager);
        let translator = self.translator.clone();
        // the page waits for `done`: it must come even if the thread cannot start
        let done = Arc::new(std::sync::Mutex::new(Some(done)));
        let finish = Arc::clone(&done);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-import".into())
            .spawn(move || {
                tracing::info!(zip = %zip.display(), "importing a dictionary");
                let result = manager
                    .import(&zip, replace.as_deref(), &cancel, progress)
                    .map_err(|e| translator.error(&e));
                if let Err(e) = &result {
                    tracing::warn!(zip = %zip.display(), "import failed: {e}");
                }
                if let Some(done) = finish.lock().ok().and_then(|mut d| d.take()) {
                    done(result);
                }
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the import thread: {e}");
            if let Some(done) = done.lock().ok().and_then(|mut d| d.take()) {
                done(Err(e.to_string()));
            }
        }
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.tr(self.manager.set_enabled(id, enabled))
    }

    fn reorder(&self, ids: &[String]) -> Result<(), String> {
        self.tr(self.manager.reorder(ids))
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        self.tr(self.manager.delete(id))
    }

    fn rename(&self, id: &str, name: Option<String>) -> Result<(), String> {
        self.tr(self.manager.rename(id, name))
    }

    fn set_collapse(&self, id: &str, collapse: RecordCollapse) -> Result<(), String> {
        self.tr(self.manager.set_collapse(id, collapse))
    }
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF | 0xF900..=0xFAFF)
}

fn render(
    manager: &DictionaryManager,
    text: &str,
    max_results: usize,
) -> Result<Reply, DictionaryError> {
    let lookup = manager.lookup()?;
    let entries = lookup.lookup(text, Options { max_results })?;
    // a single kanji also shows its kanji entries (LKP-037)
    let mut chars = text.chars();
    let kanji = match (chars.next(), chars.next()) {
        (Some(c), None) if is_han(c) => lookup.kanji(text)?,
        _ => Vec::new(),
    };
    let records = manager.list()?;
    let dicts: Vec<DictView> = lookup
        .dictionaries()
        .iter()
        .map(|d| {
            let record = records.iter().find(|(id, _)| *id == d.id).map(|(_, r)| r);
            let styles = d.store.info().styles.clone();
            let user_css = record.map(|r| r.user_css.as_str()).unwrap_or_default();
            DictView {
                name: d.name.clone(),
                collapse: match record.map(|r| r.collapse) {
                    Some(RecordCollapse::Expanded) => Collapse::Expanded,
                    Some(RecordCollapse::Collapsed) => Collapse::Collapsed,
                    _ => Collapse::Automatic,
                },
                sheet: Sheet::parse(&format!("{styles}\n{user_css}")),
            }
        })
        .collect();
    let tags = |i: usize, name: &str| {
        lookup
            .dictionaries()
            .get(i)
            .and_then(|d| d.store.tag(name).ok().flatten())
    };
    let media = |i: usize, path: &str| {
        lookup
            .dictionaries()
            .get(i)
            .and_then(|d| d.store.media(path).ok().flatten())
    };
    let content = popup_view::content(&entries, &kanji, &dicts, &tags, &media);
    Ok(Reply {
        content: Arc::new(content),
        best_length: nikomiru_dict::lookup::best_length(&entries),
        empty: entries.is_empty() && kanji.is_empty(),
        entries: entries.len(),
    })
}
