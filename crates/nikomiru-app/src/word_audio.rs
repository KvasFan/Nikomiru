// SPDX-License-Identifier: GPL-3.0-only

//! The `word-audio` worker (AUD-001…AUD-008): the sources asked in their
//! order, or all at once for the menu, the files kept in the cache, and the
//! word played on an audio-only mpv core of its own, one job at a time, off
//! the UI thread.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use nikomiru_core::config::{AudioSourceConfig, WordAudioConfig};
use nikomiru_online::audio::{CACHE_LIMIT, Cache, Resolver, WordAudio};
use nikomiru_player::word::WordPlayer;
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::word_audio::{WordAudioItem, WordAudioService};

/// What the worker needs.
pub struct Parts {
    /// `[word_audio]` of the settings.
    pub config: WordAudioConfig,
    /// `[network] proxy` (SET-007).
    pub proxy: String,
    /// Where the audio is kept (AUD-008).
    pub cache_dir: PathBuf,
    /// The audio output of the settings (`ao` of the mpv options), if any.
    pub ao: Option<String>,
    /// The interface strings, for the messages.
    pub translator: Translator,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("sources", &self.config.sources.len())
            .finish_non_exhaustive()
    }
}

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

enum Job {
    Play(String, String, Done<bool>),
    List(String, String, Done<Vec<WordAudioItem>>),
    PlayUrl(String, Done<()>),
    Configure(Box<(WordAudioConfig, String)>),
}

/// The service: the jobs go to the worker thread, in order.
pub struct WordAudioWorker {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    /// Set when Nikomiru closes: the jobs still queued are not run.
    closing: Arc<AtomicBool>,
    unavailable: String,
}

impl std::fmt::Debug for WordAudioWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WordAudioWorker").finish_non_exhaustive()
    }
}

impl Drop for WordAudioWorker {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::SeqCst);
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Worker {
    parts: Parts,
    resolver: Result<Resolver, String>,
    cache: Cache,
    player: Option<WordPlayer>,
}

fn sources(config: &WordAudioConfig) -> Vec<AudioSourceConfig> {
    config
        .sources
        .iter()
        .filter(|s| s.enabled && !s.url.trim().is_empty())
        .cloned()
        .collect()
}

impl Worker {
    fn new(parts: Parts) -> Self {
        let resolver = Resolver::new(&parts.proxy).map_err(|e| parts.translator.error(&e));
        let cache = Cache::new(parts.cache_dir.clone(), CACHE_LIMIT);
        Self {
            parts,
            resolver,
            cache,
            player: None,
        }
    }

    fn resolver(&self) -> Result<&Resolver, String> {
        self.resolver.as_ref().map_err(Clone::clone)
    }

    /// The file of an audio: from the cache, or fetched and kept.
    fn file(&self, audio: &WordAudio) -> Result<PathBuf, String> {
        if let Some(path) = self.cache.get(&audio.url) {
            return Ok(path);
        }
        let bytes = match &audio.bytes {
            Some(bytes) => bytes.clone(),
            None => self
                .resolver()?
                .fetch(&audio.url)
                .map_err(|e| self.parts.translator.error(&e))?,
        };
        self.cache
            .put(&audio.url, &bytes)
            .map_err(|e| e.to_string())
    }

    fn play_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        if self.player.is_none() {
            let player = WordPlayer::new(self.parts.ao.as_deref())
                .map_err(|e| self.parts.translator.error(&e))?;
            self.player = Some(player);
        }
        let volume = f64::from(self.parts.config.volume.min(100));
        match &self.player {
            Some(player) => player
                .play(path, volume)
                .map_err(|e| self.parts.translator.error(&e)),
            None => Ok(()),
        }
    }

    /// AUD-004: the first audio of the first source that has one.
    fn play(&mut self, term: &str, reading: &str) -> Result<bool, String> {
        for source in sources(&self.parts.config) {
            let found = match self
                .resolver()?
                .query(&source.name, &source.url, term, reading)
            {
                Ok(found) => found,
                Err(e) => {
                    tracing::info!("word audio, {}: {e}", source.name);
                    continue;
                }
            };
            for audio in found {
                match self.file(&audio) {
                    Ok(path) => {
                        self.play_file(&path)?;
                        return Ok(true);
                    }
                    Err(e) => tracing::info!("word audio, {}: {e}", source.name),
                }
            }
        }
        Ok(false)
    }

    /// AUD-004: every source at once, their audio in their order; an audio
    /// a source gave itself is kept at once, so the menu can play it.
    fn list(&self, term: &str, reading: &str) -> Result<Vec<WordAudioItem>, String> {
        let resolver = self.resolver()?;
        let sources = sources(&self.parts.config);
        let answers: Vec<Vec<WordAudio>> = std::thread::scope(|scope| {
            let asks: Vec<_> = sources
                .iter()
                .map(|s| scope.spawn(move || resolver.query(&s.name, &s.url, term, reading)))
                .collect();
            asks.into_iter()
                .zip(&sources)
                .map(|(ask, s)| match ask.join() {
                    Ok(Ok(found)) => found,
                    Ok(Err(e)) => {
                        tracing::info!("word audio, {}: {e}", s.name);
                        Vec::new()
                    }
                    Err(_) => Vec::new(),
                })
                .collect()
        });
        let mut items: Vec<WordAudioItem> = Vec::new();
        for audio in answers.into_iter().flatten() {
            if let Some(bytes) = &audio.bytes
                && let Err(e) = self.cache.put(&audio.url, bytes)
            {
                tracing::info!("word audio cache: {e}");
            }
            if items.iter().any(|i| i.url == audio.url) {
                continue;
            }
            let label = if audio.name.is_empty() || audio.name == audio.source {
                audio.source.clone()
            } else {
                format!("{}: {}", audio.source, audio.name)
            };
            items.push(WordAudioItem {
                label,
                url: audio.url,
            });
        }
        Ok(items)
    }

    fn play_url(&mut self, url: &str) -> Result<(), String> {
        let audio = WordAudio {
            source: String::new(),
            name: String::new(),
            url: url.to_string(),
            bytes: None,
        };
        let path = self.file(&audio)?;
        self.play_file(&path)
    }
}

impl WordAudioWorker {
    /// Starts the worker.
    pub fn start(parts: Parts) -> Self {
        let unavailable = parts.translator.tr("word-audio-unavailable", &[]);
        let closing = Arc::new(AtomicBool::new(false));
        let (jobs, rx) = mpsc::channel::<Job>();
        let stop = Arc::clone(&closing);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-word-audio".into())
            .spawn(move || {
                let mut worker = Worker::new(parts);
                for job in rx {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match job {
                        Job::Play(term, reading, done) => done(worker.play(&term, &reading)),
                        Job::List(term, reading, done) => done(worker.list(&term, &reading)),
                        Job::PlayUrl(url, done) => done(worker.play_url(&url)),
                        Job::Configure(settings) => {
                            let (config, proxy) = *settings;
                            if proxy != worker.parts.proxy {
                                worker.resolver = Resolver::new(&proxy)
                                    .map_err(|e| worker.parts.translator.error(&e));
                                worker.parts.proxy = proxy;
                            }
                            worker.parts.config = config;
                        }
                    }
                }
            });
        let (jobs, worker) = match spawned {
            Ok(worker) => (Some(jobs), Some(worker)),
            Err(e) => {
                tracing::error!("cannot start the word audio thread: {e}");
                (None, None)
            }
        };
        Self {
            jobs,
            worker,
            closing,
            unavailable,
        }
    }

    fn send(&self, job: Job) {
        let refused = match &self.jobs {
            Some(jobs) => jobs.send(job).err().map(|e| e.0),
            None => Some(job),
        };
        let no = self.unavailable.clone();
        match refused {
            Some(Job::Play(_, _, done)) => done(Err(no)),
            Some(Job::List(_, _, done)) => done(Err(no)),
            Some(Job::PlayUrl(_, done)) => done(Err(no)),
            Some(Job::Configure(_)) | None => {}
        }
    }
}

impl WordAudioService for WordAudioWorker {
    fn play(&self, term: String, reading: String, done: Done<bool>) {
        self.send(Job::Play(term, reading, done));
    }

    fn list(&self, term: String, reading: String, done: Done<Vec<WordAudioItem>>) {
        self.send(Job::List(term, reading, done));
    }

    fn play_url(&self, url: String, done: Done<()>) {
        self.send(Job::PlayUrl(url, done));
    }

    fn configure(&self, config: WordAudioConfig, proxy: String) {
        self.send(Job::Configure(Box::new((config, proxy))));
    }
}
