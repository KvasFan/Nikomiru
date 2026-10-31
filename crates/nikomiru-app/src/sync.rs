// SPDX-License-Identifier: GPL-3.0-only

//! The `sync` worker (SPECIFICATION §6.14): backups, restores and the sync
//! with a WebDAV folder on a thread of their own, so that nothing waits for
//! them (BAK-009).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use nikomiru_core::Clock;
use nikomiru_core::config::{Config, SyncConfig};
use nikomiru_library::Library;
use nikomiru_library::backup::{self, Contents, Manifest};
use nikomiru_library::sync::{DecodeError, DeviceState};
use nikomiru_online::OnlineError;
use nikomiru_online::webdav::WebDav;
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::sync::{Options, SyncService, Synced};

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The longest Nikomiru waits for the last sync when it closes (BAK-009).
const CLOSE_LIMIT: Duration = Duration::from_secs(30);

/// What the worker needs.
pub struct Parts {
    /// `[sync]` of the settings.
    pub sync: SyncConfig,
    /// `[network] proxy`.
    pub proxy: String,
    /// `[network] device_id`.
    pub device_id: String,
    /// The library, if it could be opened.
    pub library: Option<Arc<Library>>,
    /// `$XDG_DATA_HOME/nikomiru`.
    pub data_dir: PathBuf,
    /// The folder of the mpv presets.
    pub presets_dir: PathBuf,
    /// The interface strings, for the messages.
    pub translator: Translator,
    /// Time source.
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("sync", &self.sync)
            .finish_non_exhaustive()
    }
}

enum Job {
    Configure(SyncConfig, String, String),
    Check(Done<()>),
    Sync(Done<Synced>),
    Backup(PathBuf, Box<Config>, Options, Done<Manifest>),
    Inspect(PathBuf, Done<Manifest>),
    Restore(PathBuf, Box<Config>, Done<()>),
    /// The last sync, if automatic.
    Close(mpsc::Sender<()>),
}

/// The service.
pub struct SyncWorker {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for SyncWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncWorker").finish_non_exhaustive()
    }
}

impl Drop for SyncWorker {
    /// The last sync, for at most [`CLOSE_LIMIT`]: past it, its thread is left
    /// to end with the process.
    fn drop(&mut self) {
        let (done, wait) = mpsc::channel();
        let sent = self
            .jobs
            .as_ref()
            .is_some_and(|j| j.send(Job::Close(done)).is_ok());
        drop(self.jobs.take());
        if sent
            && wait.recv_timeout(CLOSE_LIMIT).is_ok()
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }
}

impl SyncWorker {
    /// Starts the worker; with "Sync at start and close", it syncs at once.
    pub fn start(parts: Parts) -> Self {
        let automatic = parts.sync.automatic;
        let (jobs, rx) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("nikomiru-sync".into())
            .spawn(move || {
                let mut worker = Worker { parts };
                if automatic {
                    worker.automatic();
                }
                for job in rx {
                    worker.run(job);
                }
            });
        let (jobs, worker) = match spawned {
            Ok(worker) => (Some(jobs), Some(worker)),
            Err(e) => {
                tracing::error!("the sync worker did not start: {e}");
                (None, None)
            }
        };
        Self { jobs, worker }
    }

    fn send(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }
}

impl SyncService for SyncWorker {
    fn configure(&self, sync: SyncConfig, proxy: String, device_id: String) {
        self.send(Job::Configure(sync, proxy, device_id));
    }
    fn check(&self, done: Done<()>) {
        self.send(Job::Check(done));
    }
    fn sync(&self, done: Done<Synced>) {
        self.send(Job::Sync(done));
    }
    fn backup(&self, path: PathBuf, config: Box<Config>, options: Options, done: Done<Manifest>) {
        self.send(Job::Backup(path, config, options, done));
    }
    fn inspect(&self, path: PathBuf, done: Done<Manifest>) {
        self.send(Job::Inspect(path, done));
    }
    fn restore(&self, path: PathBuf, config: Box<Config>, done: Done<()>) {
        self.send(Job::Restore(path, config, done));
    }
}

struct Worker {
    parts: Parts,
}

fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension(backup::EXTENSION)
    }
}

impl Worker {
    fn run(&mut self, job: Job) {
        match job {
            Job::Configure(sync, proxy, device_id) => {
                self.parts.sync = sync;
                self.parts.proxy = proxy;
                self.parts.device_id = device_id;
            }
            Job::Check(done) => done(self.check()),
            Job::Sync(done) => done(self.sync()),
            Job::Backup(path, config, options, done) => {
                done(self.backup(with_extension(path), &config, options));
            }
            Job::Inspect(path, done) => {
                done(backup::inspect(&path).map_err(|e| self.parts.translator.error(&e)));
            }
            Job::Restore(path, config, done) => done(self.restore(&path, &config)),
            Job::Close(done) => {
                if self.parts.sync.automatic {
                    self.automatic();
                }
                let _ = done.send(());
            }
        }
    }

    fn now(&self) -> i64 {
        self.parts
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn message(&self, e: &OnlineError) -> String {
        self.parts.translator.error(e)
    }

    fn dav(&self) -> Result<WebDav, OnlineError> {
        let s = &self.parts.sync;
        WebDav::new(
            &s.url,
            &s.user,
            &s.password,
            s.allow_unencrypted,
            &self.parts.proxy,
        )
    }

    fn check(&self) -> Result<(), String> {
        self.dav()
            .and_then(|dav| dav.prepare())
            .map_err(|e| self.message(&e))
    }

    fn automatic(&self) {
        if self.parts.sync.url.is_empty() || self.parts.device_id.is_empty() {
            return;
        }
        if let Err(e) = self.sync() {
            tracing::info!("sync: {e}");
        }
    }

    /// Everything read and merged first, the library written after, this
    /// device's file last: a failure changes nothing (BAK-009).
    fn sync(&self) -> Result<Synced, String> {
        let tr = &self.parts.translator;
        let Some(library) = &self.parts.library else {
            return Err(tr.tr("error-library-unavailable", &[]));
        };
        let dav = self.dav().map_err(|e| self.message(&e))?;
        dav.prepare().map_err(|e| self.message(&e))?;
        let now = self.now();
        let library_error = |e: nikomiru_library::LibraryError| tr.error(&e);
        let mut merged = library.export_state(now).map_err(library_error)?;
        let devices = dav.devices().map_err(|e| self.message(&e))?;
        let (mut read, mut skipped) = (0, 0);
        for device in devices.iter().filter(|d| **d != self.parts.device_id) {
            let bytes = dav.read(device).map_err(|e| self.message(&e))?;
            match DeviceState::decode(&bytes) {
                Ok(state) => {
                    merged.merge(&state);
                    read += 1;
                }
                Err(e @ (DecodeError::Damaged | DecodeError::Newer(_))) => {
                    tracing::warn!("{device}: {e}");
                    skipped += 1;
                }
            }
        }
        library.apply_state(&merged).map_err(library_error)?;
        let mine = library.export_state(now).map_err(library_error)?;
        let bytes = mine
            .encode()
            .map_err(|e| self.message(&OnlineError::Io(e.to_string())))?;
        dav.write(&self.parts.device_id, bytes)
            .map_err(|e| self.message(&e))?;
        Ok(Synced {
            devices: read,
            skipped,
        })
    }

    fn backup(&self, path: PathBuf, config: &Config, options: Options) -> Result<Manifest, String> {
        let tr = &self.parts.translator;
        let Some(library) = &self.parts.library else {
            return Err(tr.tr("error-library-unavailable", &[]));
        };
        let settings = if options.secrets {
            config.clone()
        } else {
            config.without_secrets()
        };
        let text = settings.to_text().map_err(|e| tr.error(&e))?;
        backup::create(
            &Contents {
                config: text,
                library,
                data_dir: &self.parts.data_dir,
                presets_dir: &self.parts.presets_dir,
                dictionaries: options.dictionaries,
                secrets: options.secrets,
                version: env!("CARGO_PKG_VERSION"),
                now: self.now(),
            },
            &path,
        )
        .map_err(|e| tr.error(&e))
    }

    /// Unpacks for the next start, with the keys a backup left out kept
    /// (BAK-003).
    fn restore(&self, path: &std::path::Path, current: &Config) -> Result<(), String> {
        let tr = &self.parts.translator;
        let unpacked = backup::unpack(path, &self.parts.data_dir).map_err(|e| tr.error(&e))?;
        let text = std::fs::read_to_string(&unpacked).map_err(|e| e.to_string())?;
        let mut config = Config::from_toml(&text).map_err(|e| e.to_string())?;
        config.fill_secrets(current);
        config.save(&unpacked).map_err(|e| tr.error(&e))
    }
}
