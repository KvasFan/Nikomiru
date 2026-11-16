// SPDX-License-Identifier: GPL-3.0-only

//! Application updates from the interface (UPD-001…UPD-004): the state of
//! the About section and the app's update worker.

use std::path::PathBuf;

/// What a job comes to, on the worker's thread.
pub type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// Who replaces this program (UPD-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// Flatpak.
    Flatpak,
    /// A distribution package.
    Package,
    /// A folder the user cannot write.
    ReadOnly,
    /// A portable install Nikomiru may replace.
    Portable,
}

/// A newer release, from a verified manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    /// Its version.
    pub release: String,
    /// When it was published.
    pub published: String,
    /// Its notes page.
    pub notes_url: String,
    /// The artifact for this platform, if any.
    pub artifact: Option<nikomiru_online::updates::Artifact>,
}

/// The app's update worker. Errors are translated messages.
pub trait UpdateService: Send {
    /// Who owns this install.
    fn owner(&self) -> Owner;
    /// Whether this build can verify a feed (it has a public key).
    fn configured(&self) -> bool;
    /// Checks `channel`: `None` when up to date.
    fn check(&self, channel: String, proxy: String, done: Done<Option<Available>>);
    /// Downloads the release's artifact; `progress` now and then.
    fn download(
        &self,
        available: Available,
        proxy: String,
        progress: Box<dyn Fn(u64, Option<u64>) + Send>,
        done: Done<PathBuf>,
    );
    /// Installs a downloaded artifact over the portable install.
    fn install(&self, archive: PathBuf, available: Available, done: Done<()>);
}

/// No update worker: a build without a key.
#[derive(Debug, Default)]
pub struct NoUpdates;

impl UpdateService for NoUpdates {
    fn owner(&self) -> Owner {
        Owner::ReadOnly
    }
    fn configured(&self) -> bool {
        false
    }
    fn check(&self, _: String, _: String, done: Done<Option<Available>>) {
        done(Err(String::new()));
    }
    fn download(
        &self,
        _: Available,
        _: String,
        _: Box<dyn Fn(u64, Option<u64>) + Send>,
        done: Done<PathBuf>,
    ) {
        done(Err(String::new()));
    }
    fn install(&self, _: PathBuf, _: Available, done: Done<()>) {
        done(Err(String::new()));
    }
}

/// The About section's update state.
#[derive(Debug, Default)]
pub struct State {
    /// A check runs.
    pub checking: bool,
    /// The last check found nothing newer.
    pub up_to_date: bool,
    /// A newer release.
    pub available: Option<Available>,
    /// A download runs: bytes and whole.
    pub downloading: Option<(u64, Option<u64>)>,
    /// The verified archive, ready to install.
    pub downloaded: Option<PathBuf>,
    /// An install runs.
    pub installing: bool,
    /// The release installed, for the next start.
    pub installed: Option<String>,
    /// The last error.
    pub error: Option<String>,
}
