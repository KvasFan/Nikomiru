// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle loader: a thread that reads subtitle files with encoding
//! detection (SUB-005, glibc iconv), extracts embedded text tracks with
//! ffmpeg (SUB-007) and aligns cues with the audio (SUB-018), so the UI thread
//! never waits. A new load cancels the extraction or alignment still running
//! for an older one.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use nikomiru_media::extract::{Extractor, Stream};
use nikomiru_media::process::Cancel;
use nikomiru_media::tools::{Search, Tools, ToolsError, probe};
use nikomiru_subs::align::{ANALYSIS_MS, AlignResult, BIN_MS, align};
use nikomiru_subs::encoding::decode;
use nikomiru_subs::{Format, parse};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::session::SubtitleSource;
use nikomiru_ui::subtitles::{AlignRequest, Loaded, Request, Source};

use crate::linux::Iconv;

type Done = Box<dyn FnOnce(Result<Loaded, String>) + Send>;
type Aligned = Box<dyn FnOnce(Result<AlignResult, String>) + Send>;

enum Task {
    Load(Request, Done),
    Align(AlignRequest, Aligned),
}

struct Job {
    task: Task,
    cancel: Cancel,
}

/// Loads subtitles on its own thread.
#[derive(Debug)]
pub struct Loader {
    jobs: mpsc::Sender<Job>,
    running: Mutex<Cancel>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.task {
            Task::Load(request, _) => f.debug_tuple("Load").field(request).finish(),
            Task::Align(request, _) => f.debug_tuple("Align").field(&request.input).finish(),
        }
    }
}

impl Loader {
    /// Starts the loader. `ffmpeg_setting` is `paths.ffmpeg`; extracted tracks
    /// are cached in `cache_dir`. Errors are translated with `translator`.
    pub fn start(translator: Translator, ffmpeg_setting: String, cache_dir: PathBuf) -> Self {
        let (jobs, rx) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("subtitles".into())
            .spawn(move || {
                let mut worker = Worker {
                    translator,
                    search: Search::from_env(&ffmpeg_setting),
                    tools: None,
                    cache_dir,
                };
                for job in rx {
                    match job.task {
                        Task::Load(request, done) => done(worker.load(&request, &job.cancel)),
                        Task::Align(request, done) => done(worker.align(&request, &job.cancel)),
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!("subtitle loader thread: {e}");
        }
        Self {
            jobs,
            running: Mutex::new(Cancel::new()),
        }
    }
}

impl SubtitleSource for Loader {
    fn load(&self, request: Request, done: Done) {
        let cancel = Cancel::new();
        if let Ok(mut running) = self.running.lock() {
            running.cancel();
            *running = cancel.clone();
        }
        let _ = self.jobs.send(Job {
            task: Task::Load(request, done),
            cancel,
        });
    }

    fn align(&self, request: AlignRequest, done: Aligned) {
        // a load after it makes it pointless: it shares the load's token
        let cancel = match self.running.lock() {
            Ok(running) => running.clone(),
            Err(_) => Cancel::new(),
        };
        let _ = self.jobs.send(Job {
            task: Task::Align(request, done),
            cancel,
        });
    }
}

struct Worker {
    translator: Translator,
    search: Search,
    /// ffmpeg, probed the first time a track is extracted.
    tools: Option<Result<Tools, ToolsError>>,
    cache_dir: PathBuf,
}

/// The format of a subtitle file from its extension (SRT if unknown).
fn format_of(path: &Path) -> Format {
    path.extension()
        .and_then(|e| Format::from_extension(&e.to_string_lossy()))
        .unwrap_or(Format::Srt)
}

impl Worker {
    fn load(&mut self, request: &Request, cancel: &Cancel) -> Result<Loaded, String> {
        let loaded = match &request.source {
            Source::File(path) => self.file(path, request.encoding.as_deref())?,
            Source::Embedded {
                ff_index, codec, ..
            } => {
                let path = self.extract(request, *ff_index, codec, cancel)?;
                // ffmpeg writes UTF-8
                let mut loaded = self.file(&path, Some("UTF-8"))?;
                loaded.encoding = None;
                loaded
            }
            Source::Bitmap(_) => return Err(self.translator.tr("error-subtitle-bitmap", &[])),
        };
        if loaded.track.cues.is_empty() {
            return Err(self.translator.tr("error-subtitle-empty", &[]));
        }
        Ok(loaded)
    }

    fn file(&self, path: &Path, forced: Option<&str>) -> Result<Loaded, String> {
        let name = || path.display().to_string();
        let bytes = std::fs::read(path).map_err(|e| {
            self.translator.tr(
                "error-subtitle-read",
                &[("path", name()), ("reason", e.to_string())],
            )
        })?;
        let decoded = decode(&bytes, forced, &Iconv).ok_or_else(|| {
            self.translator
                .tr("error-subtitle-encoding", &[("path", name())])
        })?;
        tracing::info!("subtitles {} ({})", path.display(), decoded.encoding);
        Ok(Loaded {
            track: parse(format_of(path), &decoded.text),
            encoding: Some(decoded.encoding),
        })
    }

    /// The offset of the cues against the audio of the first 20 minutes.
    fn align(&mut self, request: &AlignRequest, cancel: &Cancel) -> Result<AlignResult, String> {
        let tools = self
            .tools
            .get_or_insert_with(|| probe(&self.search))
            .as_ref()
            .map_err(|e| self.translator.error(e))?;
        let levels = nikomiru_media::energy::levels(
            &tools.ffmpeg,
            &request.input,
            request.stream,
            ANALYSIS_MS,
            BIN_MS,
            cancel,
        )
        .map_err(|e| self.translator.error(&e))?;
        Ok(align(&request.cues, &levels))
    }

    fn extract(
        &mut self,
        request: &Request,
        ff_index: i64,
        codec: &str,
        cancel: &Cancel,
    ) -> Result<PathBuf, String> {
        let tools = self
            .tools
            .get_or_insert_with(|| probe(&self.search))
            .as_ref()
            .map_err(|e| self.translator.error(e))?;
        let extractor = Extractor {
            ffmpeg: tools.ffmpeg.clone(),
            cache_dir: self.cache_dir.clone(),
        };
        let stream = Stream {
            ff_index,
            codec: codec.to_string(),
        };
        let duration = Duration::from_millis(request.duration_ms.max(0) as u64);
        let files = extractor
            .extract(&request.video, &[stream], duration, cancel)
            .map_err(|e| self.translator.error(&e))?;
        files
            .into_iter()
            .next()
            .map(|(_, path)| path)
            .ok_or_else(|| self.translator.tr("error-subtitle-empty", &[]))
    }
}
