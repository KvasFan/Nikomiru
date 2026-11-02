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
use nikomiru_online::metadata::{
    AniList, Identified, Jikan, Limiter, MetadataSource, Source, identify,
};
use nikomiru_online::opensubtitles::OpenSubtitles;
use nikomiru_online::search::{automatic, fetch, search_all};
use nikomiru_online::{
    Candidate, OnlineError, Provider, ProviderId, SearchRequest, SubtitleFile, store,
};
use nikomiru_ui::i18n::Translator;

use crate::linux::Iconv;
use nikomiru_ui::online::{Downloaded, OnlineQuery, OnlineResults, OnlineService, PageSource};

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
    /// Where the web pages read as books are kept (WEB-001).
    pub web_dir: PathBuf,
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

/// The largest web page or HTML file read (WEB-001).
const PAGE_LIMIT: usize = 8 << 20;

/// At most `limit + 1` bytes of `path`.
fn read_limited(path: &std::path::Path, limit: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The encoding a page declares, in iconv's name: in its `Content-Type`,
/// else in a `<meta>` of its first 4 KiB (WEB-001).
fn charset(content_type: Option<&str>, bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
    let declared = content_type
        .and_then(|t| t.split("charset=").nth(1))
        .or_else(|| head.split("charset=").nth(1))?;
    let name: String = declared
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    let name = match name.as_str() {
        "shift_jis" | "shift-jis" | "sjis" | "x-sjis" | "windows-31j" | "cp932" => "CP932",
        "euc-jp" | "x-euc-jp" => "EUC-JP",
        "utf-8" | "utf8" => "UTF-8",
        "iso-2022-jp" => "ISO-2022-JP",
        "" => return None,
        other => return Some(other.to_ascii_uppercase()),
    };
    Some(name.to_string())
}

enum Job {
    Search(OnlineQuery, Done<OnlineResults>),
    Download(OnlineQuery, Box<Candidate>, Done<Downloaded>),
    Automatic(OnlineQuery, Done<Option<Downloaded>>),
    Configure(Box<(OnlineConfig, NetworkConfig)>),
    Identify(String, Done<(Source, Identified)>),
    Cover(String, Done<Vec<u8>>),
    Page(PageSource, Done<PathBuf>),
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
                        Job::Identify(title, done) => done(worker.identify(&title)),
                        Job::Cover(url, done) => done(worker.cover(&url)),
                        Job::Page(source, done) => done(worker.page(&source)),
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
            Some(Job::Identify(_, done)) => done(Err(self.unavailable.clone())),
            Some(Job::Cover(_, done)) => done(Err(self.unavailable.clone())),
            Some(Job::Page(_, done)) => done(Err(self.unavailable.clone())),
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

    fn identify(&self, title: String, done: Done<(Source, Identified)>) {
        self.send(Job::Identify(title, done));
    }

    fn cover(&self, url: String, done: Done<Vec<u8>>) {
        self.send(Job::Cover(url, done));
    }

    fn page(&self, source: PageSource, done: Done<PathBuf>) {
        self.send(Job::Page(source, done));
    }
}

struct Worker {
    parts: Parts,
    /// The providers chosen that have a key, in the order preferred (MET-011).
    providers: Vec<Box<dyn Provider>>,
    /// Why a provider with a key could not be made (a wrong address, proxy).
    broken: Vec<OnlineError>,
    /// The source of series metadata (MET-001, MET-002), or why there is
    /// none; built when first asked.
    metadata: Option<Result<Box<dyn MetadataSource>, OnlineError>>,
    /// The client of the covers.
    http: Option<Http>,
}

impl Worker {
    fn new(parts: Parts) -> Self {
        let mut worker = Self {
            parts,
            providers: Vec::new(),
            broken: Vec::new(),
            metadata: None,
            http: None,
        };
        worker.build();
        worker
    }

    /// The providers of the settings: those chosen with a key, in their order.
    fn build(&mut self) {
        self.providers.clear();
        self.broken.clear();
        self.metadata = None;
        self.http = None;
        let (online, network) = (&self.parts.online, &self.parts.network);
        let http = match Http::new(&network.proxy) {
            Ok(http) => http,
            Err(e) => {
                tracing::warn!("online: {e}");
                self.broken.push(e);
                return;
            }
        };
        self.http = Some(http.clone());
        for id in ProviderId::from_keys(&online.providers) {
            let made: Result<Box<dyn Provider>, OnlineError> = match id {
                ProviderId::Jimaku if !online.jimaku_api_key.trim().is_empty() => {
                    Jimaku::new(http.clone(), &online.jimaku_url, &online.jimaku_api_key)
                        .map(|j| Box::new(j) as Box<dyn Provider>)
                }
                ProviderId::OpenSubtitles if !online.opensubtitles_api_key.trim().is_empty() => {
                    OpenSubtitles::new(
                        http.clone(),
                        &online.opensubtitles_url,
                        &online.opensubtitles_api_key,
                        &online.opensubtitles_username,
                        &online.opensubtitles_password,
                    )
                    .map(|o| Box::new(o) as Box<dyn Provider>)
                }
                _ => continue,
            };
            match made {
                Ok(p) => self.providers.push(p),
                Err(e) => self.broken.push(e),
            }
        }
    }

    /// The series of `title` on the source of the settings (MET-001,
    /// MET-002).
    fn identify(&mut self, title: &str) -> Result<(Source, Identified), String> {
        let online = &self.parts.online;
        let source = Source::from_key(&online.metadata);
        if self
            .metadata
            .as_ref()
            .is_none_or(|m| m.as_ref().is_ok_and(|m| m.source() != source))
        {
            let http = self.http.clone().ok_or_else(|| {
                self.broken
                    .first()
                    .map_or_else(String::new, |e| self.message(e))
            })?;
            self.metadata = Some(match source {
                Source::AniList => AniList::new(http, &online.anilist_url)
                    .map(|a| Box::new(a) as Box<dyn MetadataSource>),
                Source::Jikan => Jikan::new(http, &online.jikan_url, Limiter::real())
                    .map(|j| Box::new(j) as Box<dyn MetadataSource>),
            });
        }
        let found = match self.metadata.as_mut() {
            Some(Ok(m)) => m.search(title),
            Some(Err(e)) => Err(e.clone()),
            None => return Err(String::new()),
        };
        let found = found.map_err(|e| self.message(&e))?;
        Ok((source, identify(title, found)))
    }

    /// A cover image (MET-001): HTTPS only, at most a few MiB.
    /// WEB-001: the page at an address, or an HTML file, decoded to UTF-8
    /// and saved with its address for the Books page; a page saved before is
    /// read as it is.
    fn page(&self, source: &PageSource) -> Result<PathBuf, String> {
        let tr = &self.parts.translator;
        let (bytes, content_type, address) = match source {
            PageSource::Address(address) => {
                let http = self.http.as_ref().ok_or_else(String::new)?;
                let url =
                    nikomiru_online::http::checked_url(address).map_err(|e| self.message(&e))?;
                let answer = http
                    .file(&url, &[])
                    .and_then(|r| r.ok("web"))
                    .map_err(|e| self.message(&e))?;
                (answer.body, answer.content_type, url.to_string())
            }
            PageSource::File(path) => {
                let bytes = read_limited(path, PAGE_LIMIT).map_err(|e| {
                    tr.tr(
                        "error-book-io",
                        &[("reason", format!("{}: {e}", path.display()))],
                    )
                })?;
                if bytes.starts_with(nikomiru_books::web::MARK.as_bytes()) {
                    return Ok(path.clone());
                }
                (bytes, None, path.to_string_lossy().into_owned())
            }
        };
        if bytes.len() > PAGE_LIMIT {
            return Err(tr.tr("error-book-too-large", &[]));
        }
        let declared = charset(content_type.as_deref(), &bytes);
        let decoded = nikomiru_subs::encoding::decode(&bytes, declared.as_deref(), &Iconv)
            .or_else(|| nikomiru_subs::encoding::decode(&bytes, None, &Iconv))
            .ok_or_else(|| tr.tr("error-book-not-page", &[]))?;
        let text = nikomiru_books::web::saved(&address, &decoded.text);
        // named by what it holds: the same page saved twice is one file
        let name = nikomiru_anki::media::name(text.as_bytes(), "html");
        let path = self.parts.web_dir.join(name);
        std::fs::create_dir_all(&self.parts.web_dir)
            .and_then(|()| std::fs::write(&path, text))
            .map_err(|e| tr.tr("error-book-io", &[("reason", e.to_string())]))?;
        Ok(path)
    }

    fn cover(&self, url: &str) -> Result<Vec<u8>, String> {
        let http = self.http.as_ref().ok_or_else(String::new)?;
        let url = nikomiru_online::http::checked_url(url).map_err(|e| self.message(&e))?;
        let answer = http
            .get(&url, &[])
            .and_then(|r| r.ok("cover"))
            .map_err(|e| self.message(&e))?;
        Ok(answer.body)
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
            hash: if q.folder.is_some() {
                None
            } else {
                nikomiru_online::hash::video_hash(&q.video)
                    .inspect_err(|e| tracing::warn!("{}: {e}", q.video.display()))
                    .ok()
            },
            language: self.parts.language.clone(),
            providers: self.providers.iter().map(|p| p.id()).collect(),
            anilist_id: q.anilist_id,
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
        let folder = match (&q.folder, request.hash) {
            (Some(folder), _) => folder.clone(),
            (None, Some(hash)) => store::folder(&self.parts.subtitles_dir, &hash),
            (None, None) => {
                return Err(self.message(&OnlineError::Io(format!(
                    "{} cannot be read",
                    q.video.display()
                ))));
            }
        };
        // a file of unknown language on Jimaku is Japanese (ADR 0008 §8)
        let language = candidate
            .language
            .clone()
            .or_else(|| subtitle_language(&file.name).map(str::to_string))
            .unwrap_or_else(|| self.parts.language.clone());
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
