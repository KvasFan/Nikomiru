// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle loader: a thread that reads subtitle files with encoding
//! detection (SUB-005, glibc iconv), extracts embedded text tracks with
//! ffmpeg (SUB-007), reads graphic ones as text (SUB-028) and aligns cues
//! with the audio (SUB-018), so the UI thread never waits. A new load cancels the extraction or alignment still running
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

use crate::ocr::SharedEngine;
use crate::platform::Iconv;

/// The largest subtitle file read: the same bound as a downloaded one.
const MAX_SUBTITLE_FILE: u64 = 64 << 20;
/// The largest graphic stream read: a film's is tens of MiB.
const MAX_PGS_STREAM: u64 = 1 << 30;

type Done = Box<dyn FnOnce(Result<Loaded, String>) + Send>;
type Aligned = Box<dyn FnOnce(Result<AlignResult, String>) + Send>;

enum Task {
    Load(Request, nikomiru_ui::subtitles::Reports, Done),
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
    /// The same for the secondary subtitle's loads (SUB-017).
    running_secondary: Mutex<Cancel>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.task {
            Task::Load(request, _, _) => f.debug_tuple("Load").field(request).finish(),
            Task::Align(request, _) => f.debug_tuple("Align").field(&request.input).finish(),
        }
    }
}

impl Loader {
    /// Starts the loader. `ffmpeg_setting` is `paths.ffmpeg`; extracted tracks
    /// are cached in `cache_dir`; `ocr` reads graphic tracks. Errors are
    /// translated with `translator`.
    pub fn start(
        translator: Translator,
        ffmpeg_setting: String,
        cache_dir: PathBuf,
        ocr: SharedEngine,
    ) -> Self {
        let (jobs, rx) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("subtitles".into())
            .spawn(move || {
                let mut worker = Worker {
                    translator,
                    search: Search::from_env(&ffmpeg_setting),
                    tools: None,
                    cache_dir,
                    ocr,
                };
                for job in rx {
                    match job.task {
                        Task::Load(request, reports, done) => {
                            done(worker.load(&request, &job.cancel, &reports))
                        }
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
            running_secondary: Mutex::new(Cancel::new()),
        }
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        if let Ok(cancel) = self.running.lock() {
            cancel.cancel();
        }
        if let Ok(cancel) = self.running_secondary.lock() {
            cancel.cancel();
        }
    }
}

impl SubtitleSource for Loader {
    fn load(&self, request: Request, done: Done) {
        self.load_progressive(request, Box::new(|_| {}), done);
    }

    fn load_progressive(
        &self,
        request: Request,
        reports: nikomiru_ui::subtitles::Reports,
        done: Done,
    ) {
        let cancel = Cancel::new();
        let slot = if request.secondary {
            &self.running_secondary
        } else {
            &self.running
        };
        if let Ok(mut running) = slot.lock() {
            running.cancel();
            *running = cancel.clone();
        }
        let _ = self.jobs.send(Job {
            task: Task::Load(request, reports, done),
            cancel,
        });
    }

    fn cancel(&self, secondary: bool) {
        let slot = if secondary {
            &self.running_secondary
        } else {
            &self.running
        };
        if let Ok(cancel) = slot.lock() {
            cancel.cancel();
        }
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
    ocr: SharedEngine,
}

/// The format of a subtitle file from its extension (SRT if unknown).
fn format_of(path: &Path) -> Format {
    path.extension()
        .and_then(|e| Format::from_extension(&e.to_string_lossy()))
        .unwrap_or(Format::Srt)
}

/// Reads a subtitle file, its encoding found or `forced` (SUB-005).
pub fn read_file(
    path: &Path,
    forced: Option<&str>,
    translator: &Translator,
) -> Result<Loaded, String> {
    use std::io::Read;
    let name = || path.display().to_string();
    let failed = |e: std::io::Error| {
        translator.tr(
            "error-subtitle-read",
            &[("path", name()), ("reason", e.to_string())],
        )
    };
    // a subtitle file is text: one this large is not one
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_SUBTITLE_FILE + 1).read_to_end(&mut bytes))
        .map_err(failed)?;
    if bytes.len() as u64 > MAX_SUBTITLE_FILE {
        return Err(translator.tr("error-subtitle-too-large", &[("path", name())]));
    }
    let decoded = decode(&bytes, forced, &Iconv)
        .ok_or_else(|| translator.tr("error-subtitle-encoding", &[("path", name())]))?;
    tracing::info!("subtitles {} ({})", path.display(), decoded.encoding);
    Ok(Loaded {
        track: parse(format_of(path), &decoded.text),
        encoding: Some(decoded.encoding),
        fonts: Vec::new(),
    })
}

impl Worker {
    fn load(
        &mut self,
        request: &Request,
        cancel: &Cancel,
        reports: &nikomiru_ui::subtitles::Reports,
    ) -> Result<Loaded, String> {
        let loaded = match &request.source {
            Source::File(path) if request.source.is_graphic_read() => {
                self.read_pair(request, path, cancel, reports)?
            }
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
            Source::Read {
                ff_index, codec, ..
            } => self.read_graphic(request, *ff_index, codec, cancel, reports)?,
            Source::Bitmap(_) => return Err(self.translator.tr("error-subtitle-bitmap", &[])),
        };
        if loaded.track.cues.is_empty() {
            return Err(self.translator.tr("error-subtitle-empty", &[]));
        }
        let mut loaded = loaded;
        if loaded.track.format == Format::Ass {
            loaded.fonts = self.fonts(&request.video, cancel);
        }
        Ok(loaded)
    }

    fn file(&self, path: &Path, forced: Option<&str>) -> Result<Loaded, String> {
        read_file(path, forced, &self.translator)
    }

    /// The fonts attached to the video, for an ASS track (SUB-025); none
    /// when they cannot be read.
    fn fonts(&mut self, video: &Path, cancel: &Cancel) -> Vec<PathBuf> {
        if !video.is_file() {
            return Vec::new();
        }
        let Ok(tools) = self
            .tools
            .get_or_insert_with(|| probe(&self.search))
            .as_ref()
        else {
            return Vec::new();
        };
        nikomiru_media::attachments::fonts(
            &tools.ffprobe,
            &tools.ffmpeg,
            video,
            &self.cache_dir,
            cancel,
        )
        .inspect_err(|e| tracing::warn!("fonts of {}: {e}", video.display()))
        .unwrap_or_default()
    }

    /// A PGS or VobSub track as text (SUB-028, SUB-029): read once, then
    /// from the cache.
    fn read_graphic(
        &mut self,
        request: &Request,
        ff_index: i64,
        codec: &str,
        cancel: &Cancel,
        reports: &nikomiru_ui::subtitles::Reports,
    ) -> Result<Loaded, String> {
        use std::io::Read;
        let translator = self.translator.clone();
        let io = |e: std::io::Error| {
            translator.tr(
                "error-subtitle-read",
                &[
                    ("path", request.video.display().to_string()),
                    ("reason", e.to_string()),
                ],
            )
        };
        let cached =
            crate::graphic::cache_path(&self.cache_dir, &request.video, ff_index).map_err(io)?;
        if !cached.is_file() {
            if !self.ocr.available() {
                return Err(self.translator.tr("error-subtitle-read-model", &[]));
            }
            let sup = self.extract(request, ff_index, codec, cancel)?;
            let mut bytes = Vec::new();
            std::fs::File::open(&sup)
                .and_then(|f| f.take(MAX_PGS_STREAM + 1).read_to_end(&mut bytes))
                .map_err(io)?;
            if bytes.len() as u64 > MAX_PGS_STREAM {
                return Err(self.translator.tr(
                    "error-subtitle-too-large",
                    &[("path", request.video.display().to_string())],
                ));
            }
            // SUB-029: VobSub, with the palette of its `.idx` header
            let read = if codec == nikomiru_media::extract::DVD {
                let palette = self.palette(&request.video, ff_index, cancel);
                let cues = nikomiru_subs::vobsub::parse(&bytes, palette);
                drop(bytes);
                self.read_cues(&cues, ff_index, request.position_ms, cancel, reports)?
            } else {
                let cues = nikomiru_subs::pgs::parse(&bytes);
                drop(bytes);
                self.read_cues(&cues, ff_index, request.position_ms, cancel, reports)?
            };
            if read.is_empty() {
                return Err(self.translator.tr("error-subtitle-empty", &[]));
            }
            let mut part = cached.clone().into_os_string();
            part.push(".part");
            let part = PathBuf::from(part);
            if cancel.is_cancelled() {
                return Err(self.translator.tr("error-subtitle-read-cancelled", &[]));
            }
            let saved = std::fs::write(&part, nikomiru_subs::srt::write(&read)).and_then(|()| {
                if cancel.is_cancelled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "cancelled",
                    ));
                }
                std::fs::rename(&part, &cached)
            });
            if saved.is_err() {
                let _ = std::fs::remove_file(&part);
            }
            saved.map_err(io)?;
        }
        let mut loaded = self.file(&cached, Some("UTF-8"))?;
        loaded.encoding = None;
        Ok(loaded)
    }

    fn read_pair(
        &mut self,
        request: &Request,
        selected: &Path,
        cancel: &Cancel,
        reports: &nikomiru_ui::subtitles::Reports,
    ) -> Result<Loaded, String> {
        let bad = || self.translator.tr("error-subtitle-vobsub-index", &[]);
        let io = |error: std::io::Error| {
            self.translator.tr(
                "error-subtitle-read",
                &[
                    ("path", selected.display().to_string()),
                    ("reason", error.to_string()),
                ],
            )
        };
        let (idx_path, sub_path) = crate::vobsub_pair::sources(selected).map_err(io)?;
        let bytes = crate::vobsub_pair::read(
            &idx_path,
            nikomiru_subs::vobsub_index::MAX_BYTES as u64,
            cancel,
        )
        .map_err(io)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| bad())?;
        let index = nikomiru_subs::vobsub_index::Index::parse(text).ok_or_else(bad)?;
        let track = index.choose(&request.preferred_languages).ok_or_else(bad)?;
        let cached =
            crate::vobsub_pair::cache_path(&self.cache_dir, &idx_path, &sub_path, track.index)
                .map_err(io)?;
        if !cached.is_file() {
            if !self.ocr.available() {
                return Err(self.translator.tr("error-subtitle-read-model", &[]));
            }
            let bytes = crate::vobsub_pair::read(&sub_path, MAX_PGS_STREAM, cancel).map_err(io)?;
            let cues =
                nikomiru_subs::vobsub::parse_indexed(&bytes, &index, track).ok_or_else(bad)?;
            let read = self.read_cues(
                &cues,
                i64::from(track.index),
                request.position_ms,
                cancel,
                reports,
            )?;
            if read.is_empty() {
                return Err(self.translator.tr("error-subtitle-empty", &[]));
            }
            if cancel.is_cancelled() {
                return Err(self.translator.tr("error-subtitle-read-cancelled", &[]));
            }
            let mut part = cached.clone().into_os_string();
            part.push(".part");
            let part = PathBuf::from(part);
            std::fs::create_dir_all(&self.cache_dir).map_err(io)?;
            let saved = std::fs::write(&part, nikomiru_subs::srt::write(&read)).and_then(|()| {
                if cancel.is_cancelled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "cancelled",
                    ));
                }
                std::fs::rename(&part, &cached)
            });
            if saved.is_err() {
                let _ = std::fs::remove_file(&part);
            }
            saved.map_err(io)?;
        }
        let mut loaded = self.file(&cached, Some("UTF-8"))?;
        loaded.encoding = None;
        Ok(loaded)
    }

    /// The text of a graphic track's `cues`, read on part of the cores.
    fn read_cues<C: nikomiru_subs::pgs::Graphic>(
        &self,
        cues: &[C],
        ff_index: i64,
        position_ms: i64,
        cancel: &Cancel,
        reports: &nikomiru_ui::subtitles::Reports,
    ) -> Result<Vec<crate::graphic::ReadCue>, String> {
        if cues.is_empty() {
            return Err(self.translator.tr("error-subtitle-empty", &[]));
        }
        let engine = match crate::ocr::test_text() {
            Some(_) => None,
            None => Some(
                self.ocr
                    .get()
                    .map_err(|e| self.translator.error(&e as &dyn nikomiru_core::Localized))?,
            ),
        };
        let read_line = |line: &nikomiru_subs::pgs::Picture| -> Result<String, String> {
            let Some(engine) = &engine else {
                #[cfg(feature = "test-harness")]
                if let Some(delay) = std::env::var("NIKOMIRU_TEST_OCR_DELAY_MS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    std::thread::sleep(Duration::from_millis(delay.min(2000)));
                }
                return Ok(crate::ocr::test_text().unwrap_or_default());
            };
            let picture = nikomiru_ocr::grey(line.width, line.height, line.grey.clone())
                .ok_or_else(|| "a line of no size".to_string())?;
            engine.read(&picture).map_err(|e| e.to_string())
        };
        let started = std::time::Instant::now();
        let threads = crate::graphic::threads();
        let Some(read) = crate::graphic::read_progressive(
            cues,
            position_ms,
            threads,
            cancel,
            &read_line,
            &|read, done, total| {
                reports(nikomiru_ui::subtitles::Progress {
                    loaded: Loaded {
                        track: parse(Format::Srt, &nikomiru_subs::srt::write(read)),
                        encoding: None,
                        fonts: Vec::new(),
                    },
                    done,
                    total,
                });
            },
        )
        .map_err(|reason| {
            self.translator
                .tr("error-subtitle-read-failed", &[("reason", reason)])
        })?
        else {
            return Err(self.translator.tr("error-subtitle-read-cancelled", &[]));
        };
        tracing::info!(
            "{} pictures of stream {ff_index} read as {} lines in {:?} on {threads} threads",
            cues.len(),
            read.len(),
            started.elapsed()
        );
        Ok(read)
    }

    /// The palette of VobSub stream `ff_index` (SUB-029), from its private
    /// data; `None`, and the slots as a DVD usually draws them, when it
    /// cannot be read.
    fn palette(
        &mut self,
        video: &Path,
        ff_index: i64,
        cancel: &Cancel,
    ) -> Option<nikomiru_subs::vobsub::Palette> {
        let tools = self
            .tools
            .get_or_insert_with(|| probe(&self.search))
            .as_ref()
            .ok()?;
        let data = nikomiru_media::extract::private_data(&tools.ffprobe, video, ff_index, cancel)
            .inspect_err(|e| tracing::warn!("the palette of stream {ff_index}: {e}"))
            .ok()??;
        nikomiru_subs::vobsub::palette(&String::from_utf8_lossy(&data))
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
