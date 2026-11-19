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
use nikomiru_library::crypt::{self, CryptError};
use nikomiru_library::sync::{DecodeError, DeviceState};
use nikomiru_online::OnlineError;
use nikomiru_online::webdav::{WebDav, is_media};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::sync::{Options, SyncService, Synced};

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The age from which a file of the review that no card uses is removed: a
/// card being made writes its files first (REV-006).
const ORPHAN_AGE: Duration = Duration::from_secs(600);

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
    /// The settings at start, for a backup made by itself then (BAK-011);
    /// `None`: none is made.
    pub start_config: Option<Box<Config>>,
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
    /// The backups made by themselves (BAK-011).
    Automatic(Done<Vec<(PathBuf, i64)>>),
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
                worker.automatic_backup();
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
    fn automatic_backups(&self, done: Done<Vec<(PathBuf, i64)>>) {
        self.send(Job::Automatic(done));
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
            Job::Automatic(done) => done(Ok(backup::automatic_list(
                &self.parts.data_dir.join(backup::AUTOMATIC),
            ))),
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

    /// A backup made by itself at start, when one is due; the oldest beyond
    /// those kept go (BAK-011). A failure is only logged.
    fn automatic_backup(&self) {
        let (Some(config), Some(days)) = (&self.parts.start_config, self.parts.sync.backup_days())
        else {
            return;
        };
        if self.parts.library.is_none() {
            return;
        }
        let dir = self.parts.data_dir.join(backup::AUTOMATIC);
        let now = self.now();
        if !backup::automatic_due(&dir, now, days) {
            return;
        }
        let made = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                self.backup(
                    dir.join(backup::automatic_name(now)),
                    config,
                    Options::default(),
                )
            });
        match made {
            Ok(_) => backup::automatic_prune(&dir, self.parts.sync.backups_kept()),
            Err(e) => tracing::warn!("automatic backup: {e}"),
        }
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
        let (mut read, mut skipped, mut locked) = (0, 0, 0);
        let passphrase = &self.parts.sync.passphrase;
        for device in devices.iter().filter(|d| **d != self.parts.device_id) {
            let bytes = dav.read(device).map_err(|e| self.message(&e))?;
            // BAK-012: another passphrase, or none here: skipped, and said
            let bytes = match crypt::open(passphrase, &bytes) {
                Ok(bytes) => bytes,
                Err(e @ (CryptError::NoPassphrase | CryptError::Wrong)) => {
                    tracing::warn!("{device}: {e}");
                    locked += 1;
                    continue;
                }
                Err(e) => {
                    tracing::warn!("{device}: {e}");
                    skipped += 1;
                    continue;
                }
            };
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
        // the media first: this device's file names only cards whose files
        // are on the server
        self.sync_media(&dav, library)
            .map_err(|e| self.message(&e))?;
        let mine = library.export_state(now).map_err(library_error)?;
        let bytes = mine
            .encode()
            .map_err(|e| self.message(&OnlineError::Io(e.to_string())))?;
        let bytes = crypt::seal_if(passphrase, bytes)
            .map_err(|e| self.message(&OnlineError::Io(e.to_string())))?;
        dav.write(&self.parts.device_id, bytes)
            .map_err(|e| self.message(&e))?;
        Ok(Synced {
            devices: read,
            skipped,
            locked,
        })
    }

    /// The media of the review (REV-006): what the cards here use and the
    /// server lacks goes up; what they use and only the server has comes
    /// down, kept only if its content gives its name; what no card uses any
    /// more goes, once older than any card being made.
    fn sync_media(&self, dav: &WebDav, library: &Library) -> Result<(), OnlineError> {
        let dir = self.parts.data_dir.join(backup::REVIEW);
        let needed = library
            .review_files()
            .map_err(|e| OnlineError::Io(e.to_string()))?;
        if needed.is_empty() && !dir.exists() {
            return Ok(());
        }
        let remote = dav.media()?;
        for name in needed.iter().filter(|n| is_media(n)) {
            let path = dir.join(name);
            let here = path.is_file();
            let there = remote.iter().any(|r| r == name);
            let passphrase = &self.parts.sync.passphrase;
            if here && !there {
                let bytes = std::fs::read(&path).map_err(|e| OnlineError::Io(e.to_string()))?;
                // BAK-012: sealed too; its name is its plain content's hash
                let bytes = crypt::seal_if(passphrase, bytes)
                    .map_err(|e| OnlineError::Io(e.to_string()))?;
                dav.write_media(name, bytes)?;
            } else if !here && there {
                match dav.read_media(name).and_then(|b| {
                    crypt::open(passphrase, &b).map_err(|e| OnlineError::Io(e.to_string()))
                }) {
                    Ok(bytes) => {
                        let ext = name.rsplit('.').next().unwrap_or_default();
                        if nikomiru_anki::media::name(&bytes, ext) != *name {
                            tracing::warn!("sync: {name} does not hold what its name says");
                            continue;
                        }
                        let part = dir.join(format!("{name}.part"));
                        let written = std::fs::create_dir_all(&dir)
                            .and_then(|()| std::fs::write(&part, &bytes))
                            .and_then(|()| std::fs::rename(&part, &path));
                        if let Err(e) = written {
                            let _ = std::fs::remove_file(&part);
                            tracing::warn!("sync: {name}: {e}");
                        }
                    }
                    // a file that will come at the next sync
                    Err(e) => tracing::warn!("sync: {name}: {e}"),
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let old = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > ORPHAN_AGE));
                if is_media(&name) && !needed.contains(&name) && old {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
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
