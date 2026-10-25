// SPDX-License-Identifier: GPL-3.0-only

//! The `library` worker (LIB-002…LIB-012): the scans of the imported
//! folders, the covers made with ffmpeg, the reads of the home page and the
//! user's edits, one at a time on a thread of their own, never on the UI
//! thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use nikomiru_library::import::ImportReport;
use nikomiru_library::{Grouping, Library, VideoRecord};
use nikomiru_media::process::Cancel;
use nikomiru_ui::catalog::{Cover, Home, LibraryEdit, LibraryService, SESSIONS_SHOWN, Stats};
use nikomiru_ui::i18n::Translator;

/// How many videos "Continue watching" shows.
const CONTINUE: usize = 12;

/// What the worker needs.
pub struct Parts {
    /// The library.
    pub library: Arc<Library>,
    /// `[paths] ffmpeg` of the settings, to find ffmpeg and ffprobe.
    pub ffmpeg: String,
    /// The cache, where the covers are kept.
    pub cache_dir: PathBuf,
    /// The interface strings, for the messages.
    pub translator: Translator,
    /// The clock, for the times of the scans.
    pub clock: Arc<dyn nikomiru_core::Clock>,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("cache_dir", &self.cache_dir)
            .finish_non_exhaustive()
    }
}

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

enum Job {
    Home(Done<Home>),
    Episodes(u64, Done<Vec<(String, VideoRecord)>>),
    Import(PathBuf, Grouping, Done<ImportReport>),
    Edit(LibraryEdit, Done<()>),
    Cover(u64, Done<Cover>),
    Stats(Done<Stats>),
    ExportCsv(PathBuf, Done<PathBuf>),
}

/// The service: the jobs go to the worker thread, in order.
pub struct Catalog {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    /// Set when Nikomiru closes: the jobs still queued are not run, and a
    /// running ffmpeg is stopped.
    closing: Arc<AtomicBool>,
    cancel: Cancel,
    unavailable: String,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog").finish_non_exhaustive()
    }
}

impl Drop for Catalog {
    /// The job on the worker ends; the queued ones are dropped.
    fn drop(&mut self) {
        self.closing.store(true, Ordering::SeqCst);
        self.cancel.cancel();
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Worker {
    parts: Parts,
    /// ffmpeg and ffprobe, looked for at the first cover.
    tools: Option<Result<(PathBuf, PathBuf), String>>,
    cancel: Cancel,
}

impl Worker {
    fn now(&self) -> i64 {
        self.parts
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn message(&self, e: &nikomiru_library::LibraryError) -> String {
        self.parts.translator.error(e)
    }

    fn home(&self) -> Result<Home, String> {
        let lib = &self.parts.library;
        let e = |e| self.message(&e);
        Ok(Home {
            continue_watching: lib.continue_watching(CONTINUE).map_err(e)?,
            series: lib.series_list().map_err(e)?,
            folders: lib.folders().map_err(e)?,
        })
    }

    fn import(&self, root: &Path, grouping: Grouping) -> Result<ImportReport, String> {
        if !root.is_dir() {
            return Err(self
                .parts
                .translator
                .tr("library-no-folder", &[("path", root.display().to_string())]));
        }
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let root = root.as_path();
        let files = nikomiru_library::scan::video_files(root);
        tracing::info!("{}: {} videos", root.display(), files.len());
        self.parts
            .library
            .import(root, grouping, &files, self.now())
            .map_err(|e| self.message(&e))
    }

    fn edit(&self, edit: LibraryEdit) -> Result<(), String> {
        let lib = &self.parts.library;
        let result = match edit {
            LibraryEdit::Rename(id, title) => lib.rename_series(id, &title),
            LibraryEdit::Delete(id) => lib.delete_series(id),
            LibraryEdit::Merge { from, into } => lib.merge_series(from, into),
            LibraryEdit::Move { uids, to } => uids.iter().try_for_each(|u| lib.move_episode(u, to)),
            LibraryEdit::Split { uids, title } => {
                lib.split_series(&uids, &title, self.now()).map(drop)
            }
            LibraryEdit::Remove(uids) => uids.iter().try_for_each(|u| lib.remove_video(u)),
            LibraryEdit::DeleteSession(id) => lib.delete_segment(id),
        };
        result.map_err(|e| self.message(&e))
    }

    fn tools(&mut self) -> Result<(PathBuf, PathBuf), String> {
        let setting = self.parts.ffmpeg.clone();
        let translator = &self.parts.translator;
        self.tools
            .get_or_insert_with(|| {
                nikomiru_media::tools::probe(&nikomiru_media::tools::Search::from_env(&setting))
                    .map(|t| (t.ffmpeg, t.ffprobe))
                    .map_err(|e| translator.error(&e))
            })
            .clone()
    }

    /// STA-005: the totals by day and by series, the latest sessions.
    fn stats(&self) -> Result<Stats, String> {
        let lib = &self.parts.library;
        let e = |e| self.message(&e);
        let videos: std::collections::BTreeMap<String, VideoRecord> =
            lib.videos().map_err(e)?.into_iter().collect();
        let sessions = lib
            .segments()
            .map_err(e)?
            .into_iter()
            .take(SESSIONS_SHOWN)
            .map(|(id, s)| {
                let name = videos
                    .get(&s.media)
                    .and_then(|v| Path::new(&v.path).file_stem())
                    .map_or_else(|| s.media.clone(), |n| n.to_string_lossy().into_owned());
                (id, s, name)
            })
            .collect();
        Ok(Stats {
            days: lib.day_totals().map_err(e)?,
            series: lib.series_totals().map_err(e)?,
            sessions,
        })
    }

    /// STA-005: the sessions as CSV, at `path`.
    fn export_csv(&self, path: &Path) -> Result<PathBuf, String> {
        let csv = self
            .parts
            .library
            .sessions_csv()
            .map_err(|e| self.message(&e))?;
        std::fs::write(path, csv).map_err(|e| {
            self.parts.translator.tr(
                "player-write-error",
                &[
                    ("path", path.display().to_string()),
                    ("reason", e.to_string()),
                ],
            )
        })?;
        Ok(path.to_path_buf())
    }

    /// LIB-005: the cover kept, or the frame of the first episode.
    fn cover(&mut self, series: u64) -> Result<Cover, String> {
        let lib = Arc::clone(&self.parts.library);
        let e = |e| self.message(&e);
        let record = lib.series(series).map_err(e)?.unwrap_or_default();
        let kept = record.cover.map(PathBuf::from).filter(|p| p.is_file());
        let path = match kept {
            Some(path) => path,
            None => {
                let Some((uid, first)) = lib.episodes(series).map_err(e)?.into_iter().next() else {
                    return Err(format!("series {series} has no episode"));
                };
                let path = nikomiru_media::cover::path(&self.parts.cache_dir, &uid);
                let (ffmpeg, ffprobe) = self.tools()?;
                nikomiru_media::cover::make(
                    &ffmpeg,
                    &ffprobe,
                    Path::new(&first.path),
                    &path,
                    &self.cancel,
                )
                .map_err(|e| self.parts.translator.error(&e))?;
                lib.set_cover(series, &path.to_string_lossy())
                    .map_err(|e| self.message(&e))?;
                path
            }
        };
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        Cover::decode(&bytes)
    }
}

impl Catalog {
    /// Starts the worker.
    pub fn start(parts: Parts) -> Self {
        let unavailable = parts.translator.tr("error-library-unavailable", &[]);
        let closing = Arc::new(AtomicBool::new(false));
        let cancel = Cancel::new();
        let (jobs, rx) = mpsc::channel::<Job>();
        let stop = Arc::clone(&closing);
        let worker_cancel = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("nikomiru-library".into())
            .spawn(move || {
                let mut worker = Worker {
                    parts,
                    tools: None,
                    cancel: worker_cancel,
                };
                for job in rx {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match job {
                        Job::Home(done) => done(worker.home()),
                        Job::Episodes(id, done) => done(
                            worker
                                .parts
                                .library
                                .episodes(id)
                                .map_err(|e| worker.message(&e)),
                        ),
                        Job::Import(root, grouping, done) => done(worker.import(&root, grouping)),
                        Job::Edit(edit, done) => done(worker.edit(edit)),
                        Job::Cover(id, done) => done(worker.cover(id)),
                        Job::Stats(done) => done(worker.stats()),
                        Job::ExportCsv(path, done) => done(worker.export_csv(&path)),
                    }
                }
            });
        let (jobs, worker) = match spawned {
            Ok(worker) => (Some(jobs), Some(worker)),
            Err(e) => {
                tracing::error!("cannot start the library thread: {e}");
                (None, None)
            }
        };
        Self {
            jobs,
            worker,
            closing,
            cancel,
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
            Some(Job::Home(done)) => done(Err(no)),
            Some(Job::Episodes(_, done)) => done(Err(no)),
            Some(Job::Import(_, _, done)) => done(Err(no)),
            Some(Job::Edit(_, done)) => done(Err(no)),
            Some(Job::Cover(_, done)) => done(Err(no)),
            Some(Job::Stats(done)) => done(Err(no)),
            Some(Job::ExportCsv(_, done)) => done(Err(no)),
            None => {}
        }
    }
}

impl LibraryService for Catalog {
    fn home(&self, done: Done<Home>) {
        self.send(Job::Home(done));
    }

    fn episodes(&self, series: u64, done: Done<Vec<(String, VideoRecord)>>) {
        self.send(Job::Episodes(series, done));
    }

    fn import(&self, root: PathBuf, grouping: Grouping, done: Done<ImportReport>) {
        self.send(Job::Import(root, grouping, done));
    }

    fn edit(&self, edit: LibraryEdit, done: Done<()>) {
        self.send(Job::Edit(edit, done));
    }

    fn cover(&self, series: u64, done: Done<Cover>) {
        self.send(Job::Cover(series, done));
    }

    fn stats(&self, done: Done<Stats>) {
        self.send(Job::Stats(done));
    }

    fn export_csv(&self, path: PathBuf, done: Done<PathBuf>) {
        self.send(Job::ExportCsv(path, done));
    }
}
