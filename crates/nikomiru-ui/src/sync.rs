// SPDX-License-Identifier: GPL-3.0-only

//! Backup and sync from the interface (SPECIFICATION §6.14): the jobs of the
//! app's `sync` worker, and the state of Settings → Backup.

use std::path::PathBuf;

use nikomiru_core::config::{Config, SyncConfig};
use nikomiru_library::backup::Manifest;

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// A time for people: its local day (`YYYY-MM-DD`), from Unix ms.
pub fn date_of(unix_ms: i64) -> String {
    nikomiru_library::stats::date_of(unix_ms.div_euclid(86_400_000))
}

/// How a sync went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Synced {
    /// The other devices read.
    pub devices: usize,
    /// Their files that could not be used (BAK-006).
    pub skipped: usize,
}

/// The app's `sync` worker: each call only queues. The answers come back on
/// its thread; errors are translated messages.
pub trait SyncService: Send {
    /// New settings.
    fn configure(&self, sync: SyncConfig, proxy: String, device_id: String);
    /// Whether the folder can be written (BAK-004).
    fn check(&self, done: Done<()>);
    /// Syncs now.
    fn sync(&self, done: Done<Synced>);
    /// Writes a backup with these settings.
    fn backup(&self, path: PathBuf, config: Box<Config>, options: Options, done: Done<Manifest>);
    /// What a backup holds.
    fn inspect(&self, path: PathBuf, done: Done<Manifest>);
    /// Unpacks a backup for the next start, these being the settings now.
    fn restore(&self, path: PathBuf, config: Box<Config>, done: Done<()>);
}

/// What goes into a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    /// The dictionaries (BAK-001).
    pub dictionaries: bool,
    /// Keys and passwords (BAK-002).
    pub secrets: bool,
}

/// No backup or sync: for the tests of the interface.
#[derive(Debug, Default)]
pub struct NoSync;

impl SyncService for NoSync {
    fn configure(&self, _: SyncConfig, _: String, _: String) {}
    fn check(&self, done: Done<()>) {
        done(Err(String::new()));
    }
    fn sync(&self, done: Done<Synced>) {
        done(Err(String::new()));
    }
    fn backup(&self, _: PathBuf, _: Box<Config>, _: Options, done: Done<Manifest>) {
        done(Err(String::new()));
    }
    fn inspect(&self, _: PathBuf, done: Done<Manifest>) {
        done(Err(String::new()));
    }
    fn restore(&self, _: PathBuf, _: Box<Config>, done: Done<()>) {
        done(Err(String::new()));
    }
}

/// A job for the worker, taken by the UI layer: its answer needs the waker.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncJob {
    /// See [`SyncService::check`].
    Check,
    /// See [`SyncService::sync`].
    Now,
    /// See [`SyncService::backup`].
    Backup(PathBuf, Box<Config>, Options),
    /// See [`SyncService::inspect`].
    Inspect(PathBuf),
    /// See [`SyncService::restore`].
    Restore(PathBuf, Box<Config>),
}

/// What comes back.
#[derive(Debug)]
pub enum SyncAnswer {
    /// The folder was checked.
    Checked(Result<(), String>),
    /// A sync ended.
    Synced(Result<Synced, String>),
    /// A backup was written.
    BackedUp(Result<Manifest, String>),
    /// What a chosen backup holds.
    Inspected(PathBuf, Result<Manifest, String>),
    /// A backup waits for the next start.
    Restored(Result<(), String>),
}

/// Settings → Backup.
#[derive(Debug, Default)]
pub struct BackupPage {
    /// An answer is awaited.
    pub busy: bool,
    /// The last message, and whether it is good news.
    pub status: Option<(String, bool)>,
    /// What the next backup holds.
    pub options: Options,
    /// A backup chosen, to confirm.
    pub chosen: Option<(PathBuf, Manifest)>,
}
