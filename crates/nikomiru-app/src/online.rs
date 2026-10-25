// SPDX-License-Identifier: GPL-3.0-only

//! The `online` worker ([ADR 0008] §13): the requests to the subtitle
//! providers one at a time on a thread of their own, and the downloaded files
//! kept in the data folder (MET-008).
//!
//! [ADR 0008]: ../../../docs/adr/0008-online-subtitles.md

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use nikomiru_core::config::{NetworkConfig, OnlineConfig};
use nikomiru_online::http::Http;
use nikomiru_online::jimaku::Jimaku;
use nikomiru_online::language::subtitle_language;
use nikomiru_online::opensubtitles::OpenSubtitles;
use nikomiru_online::search::{automatic, fetch, search_all};
use nikomiru_online::{Candidate, OnlineError, Provider, SearchRequest, SubtitleFile, store};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::online::{Downloaded, OnlineQuery, OnlineResults, OnlineService};

/// What the worker needs.
pub struct Parts {
    /// `[online]` of the settings.
    pub online: OnlineConfig,
    /// `[network]` of the settings.
    pub network: NetworkConfig,
    /// The study language (`[general] study_language`).
    pub language: String,
    /// Where downloaded subtitles are kept.
    pub subtitles_dir: PathBuf,
    /// The interface strings, for the messages.
    pub translator: Translator,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("online", &self.online)
            .finish_non_exhaustive()
    }
}

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

enum Job {
    Search(OnlineQuery, Done<OnlineResults>),
    Download(OnlineQuery, Box<Candidate>, Done<Downloaded>),
    Automatic(OnlineQuery, Done<Option<Downloaded>>),
    Configure(Box<(OnlineConfig, NetworkConfig)>),
}

/// The service: the jobs go to the worker thread, in order.
pub struct Online {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    /// Set when Nikomiru closes: the jobs still queued are not run.
    closing: Arc<AtomicBool>,
    unavailable: String,
}

impl std::fmt::Debug for Online {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Online").finish_non_exhaustive()
    }
}

impl Drop for Online {
    /// The job on the worker ends; the queued ones are dropped.
    fn drop(&mut self) {
        self.closing.store(true, Ordering::SeqCst);
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Online {
    /// Starts the worker.
    pub fn start(parts: Parts) -> Self {
        let unavailable = parts.translator.tr("online-no-providers", &[]);
        let closing = Arc::new(AtomicBool::new(false));
        let (jobs, rx) = mpsc::channel::<Job>();
        let stop = Arc::clone(&closing);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-online".into())
            .spawn(move || {
                let mut worker = Worker::new(parts);
                for job in rx {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match job {
                        Job::Search(q, done) => done(worker.search(&q)),
                        Job::Download(q, c, done) => done(worker.download(&q, &c)),
                        Job::Automatic(q, done) => done(worker.automatic(&q)),
                        Job::Configure(settings) => {
                            let (online, network) = *settings;
                            worker.parts.online = online;
                            worker.parts.network = network;
                            worker.build();
                        }
                    }
                }
            });
        let (jobs, worker) = match spawned {
            Ok(worker) => (Some(jobs), Some(worker)),
            Err(e) => {
                tracing::error!("cannot start the online thread: {e}");
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
        match refused {
            Some(Job::Search(_, done)) => done(Err(self.unavailable.clone())),
            Some(Job::Download(_, _, done)) => done(Err(self.unavailable.clone())),
            Some(Job::Automatic(_, done)) => done(Err(self.unavailable.clone())),
            Some(Job::Configure(_)) | None => {}
        }
    }
}

impl OnlineService for Online {
    fn search(&self, query: OnlineQuery, done: Done<OnlineResults>) {
        self.send(Job::Search(query, done));
    }

    fn download(&self, query: OnlineQuery, candidate: Candidate, done: Done<Downloaded>) {
        self.send(Job::Download(query, Box::new(candidate), done));
    }

    fn automatic(&self, query: OnlineQuery, done: Done<Option<Downloaded>>) {
        self.send(Job::Automatic(query, done));
    }

    fn configure(&self, online: OnlineConfig, network: NetworkConfig) {
        self.send(Job::Configure(Box::new((online, network))));
    }
}

struct Worker {
    parts: Parts,
    /// The providers with a key.
    providers: Vec<Box<dyn Provider>>,
    /// Why a provider with a key could not be made (a wrong address, proxy).
    broken: Vec<OnlineError>,
}

impl Worker {
    fn new(parts: Parts) -> Self {
        let mut worker = Self {
            parts,
            providers: Vec::new(),
            broken: Vec::new(),
        };
        worker.build();
        worker
    }

    /// The providers of the settings.
    fn build(&mut self) {
        self.providers.clear();
        self.broken.clear();
        let (online, network) = (&self.parts.online, &self.parts.network);
        let http = match Http::new(&network.proxy) {
            Ok(http) => http,
            Err(e) => {
                tracing::warn!("online: {e}");
                self.broken.push(e);
                return;
            }
        };
        if !online.jimaku_api_key.trim().is_empty() {
            match Jimaku::new(http.clone(), &online.jimaku_url, &online.jimaku_api_key) {
                Ok(j) => self.providers.push(Box::new(j)),
                Err(e) => self.broken.push(e),
            }
        }
        if !online.opensubtitles_api_key.trim().is_empty() {
            match OpenSubtitles::new(
                http,
                &online.opensubtitles_url,
                &online.opensubtitles_api_key,
                &online.opensubtitles_username,
                &online.opensubtitles_password,
            ) {
                Ok(o) => self.providers.push(Box::new(o)),
                Err(e) => self.broken.push(e),
            }
        }
    }

    fn message(&self, e: &OnlineError) -> String {
        self.parts.translator.error(e)
    }

    fn ready(&self) -> Result<(), String> {
        if !self.providers.is_empty() {
            return Ok(());
        }
        Err(match self.broken.first() {
            Some(e) => self.message(e),
            None => self.parts.translator.tr("online-no-providers", &[]),
        })
    }

    fn request(&self, q: &OnlineQuery) -> SearchRequest {
        SearchRequest {
            title: q.title.clone(),
            season: q.season,
            episode: q.episode,
            hash: nikomiru_online::hash::video_hash(&q.video)
                .inspect_err(|e| tracing::warn!("{}: {e}", q.video.display()))
                .ok(),
            language: self.parts.language.clone(),
        }
    }

    fn search(&mut self, q: &OnlineQuery) -> Result<OnlineResults, String> {
        self.ready()?;
        let request = self.request(q);
        let found = search_all(&mut self.providers, &request);
        Ok(OnlineResults {
            candidates: found.candidates,
            failed: found.failed.iter().map(|(_, e)| self.message(e)).collect(),
        })
    }

    /// Keeps `file` for the video (MET-008).
    fn keep(
        &self,
        q: &OnlineQuery,
        request: &SearchRequest,
        candidate: &Candidate,
        file: &SubtitleFile,
    ) -> Result<Downloaded, String> {
        let hash = request.hash.ok_or_else(|| {
            self.message(&OnlineError::Io(format!(
                "{} cannot be read",
                q.video.display()
            )))
        })?;
        // a file of unknown language on Jimaku is Japanese (ADR 0008 §8)
        let language = candidate
            .language
            .clone()
            .or_else(|| subtitle_language(&file.name).map(str::to_string))
            .unwrap_or_else(|| self.parts.language.clone());
        let folder = store::folder(&self.parts.subtitles_dir, &hash);
        let path = store::save(&folder, &q.video, &language, &file.name, &file.bytes)
            .map_err(|e| self.message(&OnlineError::Io(e.to_string())))?;
        tracing::info!(
            "subtitles from {}: {} → {}",
            candidate.provider.name(),
            file.name,
            path.display()
        );
        Ok(Downloaded {
            path,
            provider: candidate.provider,
            file_name: file.name.clone(),
        })
    }

    fn download(&mut self, q: &OnlineQuery, candidate: &Candidate) -> Result<Downloaded, String> {
        self.ready()?;
        let request = self.request(q);
        let file = fetch(&mut self.providers, candidate, &request).map_err(|e| self.message(&e))?;
        self.keep(q, &request, candidate, &file)
    }

    fn automatic(&mut self, q: &OnlineQuery) -> Result<Option<Downloaded>, String> {
        self.ready()?;
        let request = self.request(q);
        match automatic(&mut self.providers, &request).map_err(|e| self.message(&e))? {
            Some((candidate, file)) => self.keep(q, &request, &candidate, &file).map(Some),
            None => Ok(None),
        }
    }
}
