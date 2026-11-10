// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary service of the session: lookups run on a worker thread,
//! with the dictionaries of the registry (DIC-021), and come back as popup
//! content (POP-007); the subtitles are split into words on another
//! (LKP-060), so that a click never waits behind them; imports run on threads of their own, which the service
//! cancels and waits for when it goes; the page's other operations are quick
//! registry writes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::SystemTime;

use nikomiru_core::Cancel;
use nikomiru_dict::import::Progress;

use nikomiru_dict::css::Sheet;
use nikomiru_dict::lookup::Options;
use nikomiru_dict::segment::{Word, segment};
use nikomiru_library::words::Counted;
use nikomiru_library::{
    Check, Collapse as RecordCollapse, DictionaryError, DictionaryManager, DictionaryRecord,
};
use nikomiru_ui::dictionaries::{DictionaryService, Done};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::popup::Reply;
use nikomiru_ui::popup_view::{self, Collapse, DictView};
use nikomiru_ui::scores::{self, CountRequest};

/// The dictionaries of the registry: lookups on a worker thread, in order;
/// imports on threads of their own.
#[derive(Debug)]
pub struct Dictionaries {
    manager: Arc<DictionaryManager>,
    translator: Translator,
    jobs: Option<Jobs<Reply>>,
    worker: Option<JoinHandle<()>>,
    words: Option<Jobs<Vec<Word>>>,
    words_worker: Option<JoinHandle<()>>,
    /// The episodes whose words are counted (KNW-008), on a thread of
    /// their own so that the subtitles shown never wait behind them.
    counts: Option<mpsc::Sender<(CountRequest, Done<Option<Counted>>)>>,
    counts_worker: Option<JoinHandle<()>>,
    /// Set when the service goes: the episodes still to count are left.
    counts_stop: Arc<AtomicBool>,
    /// The imports that may still run, with their cancels.
    imports: Mutex<Vec<(Cancel, JoinHandle<()>)>>,
}

impl Drop for Dictionaries {
    /// The workers hold the library (through the manager): they must end
    /// before the process does, or the library is left to be repaired
    /// (REL-001). A running import is cancelled first, so the exit does not
    /// wait for it to finish (DIC-010).
    fn drop(&mut self) {
        let imports = std::mem::take(
            self.imports
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for (cancel, _) in &imports {
            cancel.cancel();
        }
        drop(self.jobs.take());
        drop(self.words.take());
        self.counts_stop.store(true, Ordering::Relaxed);
        drop(self.counts.take());
        for worker in [
            self.worker.take(),
            self.words_worker.take(),
            self.counts_worker.take(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = worker.join();
        }
        for (_, import) in imports {
            let _ = import.join();
        }
    }
}

/// The texts sent to a worker, each with what to call once done.
type Jobs<T> = mpsc::Sender<(String, Done<T>)>;

/// A thread that runs `job` on each text sent to it, and the sender.
fn spawn_worker<T: 'static>(
    name: &str,
    manager: &Arc<DictionaryManager>,
    translator: &Translator,
    job: impl Fn(&DictionaryManager, &str) -> Result<T, DictionaryError> + Send + 'static,
) -> (Jobs<T>, Option<JoinHandle<()>>) {
    let (tx, rx) = mpsc::channel::<(String, Done<T>)>();
    let manager = Arc::clone(manager);
    let translator = translator.clone();
    let spawned = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            for (text, done) in rx {
                let started = std::time::Instant::now();
                let result = job(&manager, &text).map_err(|e| translator.error(&e));
                tracing::debug!(query = %text, elapsed = ?started.elapsed(), "{}", std::thread::current().name().unwrap_or_default());
                done(result);
            }
        });
    match spawned {
        Ok(worker) => (tx, Some(worker)),
        Err(e) => {
            tracing::error!("cannot start the thread {name}: {e}");
            (tx, None)
        }
    }
}

/// The counts kept by file, size, time and the words' generation: opening
/// a series again counts nothing that did not change.
type CountCache = HashMap<(PathBuf, u64, Option<SystemTime>, u64), Counted>;

/// The counts kept at most; beyond, the cache starts again.
const COUNTS_KEPT: usize = 256;

/// The thread that counts the words of episodes (KNW-008), and its sender.
#[allow(clippy::type_complexity)]
fn spawn_counter(
    manager: &Arc<DictionaryManager>,
    translator: &Translator,
    stop: &Arc<AtomicBool>,
) -> (
    mpsc::Sender<(CountRequest, Done<Option<Counted>>)>,
    Option<JoinHandle<()>>,
) {
    let (tx, rx) = mpsc::channel::<(CountRequest, Done<Option<Counted>>)>();
    let manager = Arc::clone(manager);
    let translator = translator.clone();
    let stop = Arc::clone(stop);
    let spawned = std::thread::Builder::new()
        .name("nikomiru-count".into())
        .spawn(move || {
            let mut cache = CountCache::new();
            for (request, done) in rx {
                // the app is closing: what is queued would only delay it
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let started = std::time::Instant::now();
                let result = count_episode(&manager, &translator, &stop, &mut cache, &request);
                tracing::debug!(video = %request.video.display(), elapsed = ?started.elapsed(), "words counted");
                done(result);
            }
        });
    match spawned {
        Ok(worker) => (tx, Some(worker)),
        Err(e) => {
            tracing::error!("cannot start the thread nikomiru-count: {e}");
            (tx, None)
        }
    }
}

/// The words of the subtitle file `request`'s video would play with
/// (SUB-006, SUB-008, MET-008), or `None` when it has none.
fn count_episode(
    manager: &DictionaryManager,
    translator: &Translator,
    stop: &AtomicBool,
    cache: &mut CountCache,
    request: &CountRequest,
) -> Result<Option<Counted>, String> {
    let video = &request.video;
    if !video.is_file() {
        return Ok(None);
    }
    let downloaded = nikomiru_online::hash::video_hash(video)
        .map(|h| {
            nikomiru_online::store::list(&nikomiru_online::store::folder(
                &request.subtitles_dir,
                &h,
            ))
        })
        .unwrap_or_default();
    let externals = scores::subtitle_files(video, &downloaded);
    let Some(file) = scores::automatic_file(&externals, request.saved.as_ref(), &request.preferred)
    else {
        return Ok(None);
    };
    let meta = std::fs::metadata(&file).map_err(|e| e.to_string())?;
    let key = (file, meta.len(), meta.modified().ok(), request.generation);
    if let Some(counted) = cache.get(&key) {
        return Ok(Some(counted.clone()));
    }
    let loaded = crate::subtitles::read_file(&key.0, None, translator)?;
    let lookup = manager.lookup().map_err(|e| translator.error(&e))?;
    let lines = loaded.track.cues.into_iter().map(|c| c.plain_text);
    let counted = scores::count(lines, |text| {
        if stop.load(Ordering::Relaxed) {
            return Err("closing".into());
        }
        segment(&lookup, text).map_err(|e| translator.error(&DictionaryError::from(e)))
    })?;
    if cache.len() >= COUNTS_KEPT {
        cache.clear();
    }
    cache.insert(key, counted.clone());
    Ok(Some(counted))
}

/// Sends `text` to a worker; without one, `done` gets the error at once.
fn send<T>(jobs: Option<&Jobs<T>>, text: String, done: Done<T>, translator: &Translator) {
    let unsent = match jobs {
        Some(jobs) => jobs
            .send((text, done))
            .err()
            .map(|mpsc::SendError((_, done))| done),
        None => Some(done),
    };
    if let Some(done) = unsent {
        done(Err(translator.tr("error-dict-unavailable", &[])));
    }
}

impl Dictionaries {
    /// Starts the lookup and segmentation workers.
    pub fn start(
        manager: Arc<DictionaryManager>,
        translator: Translator,
        max_results: Arc<AtomicUsize>,
    ) -> Self {
        let (jobs, worker) =
            spawn_worker("nikomiru-lookup", &manager, &translator, move |m, text| {
                render(m, text, max_results.load(Ordering::Relaxed))
            });
        let (words, words_worker) =
            spawn_worker("nikomiru-words", &manager, &translator, |m, text| {
                Ok(segment(&*m.lookup()?, text)?)
            });
        let counts_stop = Arc::new(AtomicBool::new(false));
        let (counts, counts_worker) = spawn_counter(&manager, &translator, &counts_stop);
        Self {
            manager,
            translator,
            jobs: worker.is_some().then_some(jobs),
            worker,
            words: words_worker.is_some().then_some(words),
            words_worker,
            counts: counts_worker.is_some().then_some(counts),
            counts_worker,
            counts_stop,
            imports: Mutex::new(Vec::new()),
        }
    }

    fn tr<T>(&self, r: Result<T, DictionaryError>) -> Result<T, String> {
        r.map_err(|e| self.translator.error(&e))
    }
}

impl DictionaryService for Dictionaries {
    fn lookup(&self, text: String, done: Done<Reply>) {
        send(self.jobs.as_ref(), text, done, &self.translator);
    }

    fn segment(&self, text: String, done: Done<Vec<Word>>) {
        send(self.words.as_ref(), text, done, &self.translator);
    }

    fn count_words(&self, request: CountRequest, done: Done<Option<Counted>>) {
        let unsent = match &self.counts {
            Some(counts) => counts
                .send((request, done))
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
        let running = cancel.clone();
        // the page waits for `done`: it must come even if the thread cannot start
        let done = Arc::new(Mutex::new(Some(done)));
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
        match spawned {
            Ok(thread) => {
                let mut imports = self.imports.lock().unwrap_or_else(PoisonError::into_inner);
                imports.retain(|(_, thread)| !thread.is_finished());
                imports.push((running, thread));
            }
            Err(e) => {
                tracing::error!("cannot start the import thread: {e}");
                if let Some(done) = done.lock().ok().and_then(|mut d| d.take()) {
                    done(Err(e.to_string()));
                }
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

    fn set_user_css(&self, id: &str, css: &str) -> Result<(), String> {
        self.tr(self.manager.set_user_css(id, css))
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
