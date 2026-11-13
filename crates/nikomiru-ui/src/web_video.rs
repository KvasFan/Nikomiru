// SPDX-License-Identifier: GPL-3.0-only

//! Explicit web-video opens, cancellable resolution and relay lifetime (PLY-023).

use nikomiru_core::Cancel;
use nikomiru_online::servers::relay::Relay;
use std::{path::PathBuf, sync::Arc};

/// A requested page, with no work performed until the service is called.
#[derive(Debug, Clone)]
pub struct Request {
    /// Request generation.
    pub generation: u64,
    /// Original page URL.
    pub page: String,
    /// Executable setting; empty selects the environment or PATH.
    pub executable: String,
    /// The configured proxy (empty disables environment proxies).
    pub proxy: String,
    /// Preferred subtitle languages.
    pub languages: Vec<String>,
    /// Subtitle cache folder.
    pub folder: PathBuf,
    /// Cancellation of a superseded open.
    pub cancel: Cancel,
}

/// A resolved video. Clones retain its media relays.
#[derive(Debug, Clone)]
pub struct Ready {
    /// Original page URL, for persistent records and reopening.
    pub page: String,
    /// Human title.
    pub title: String,
    /// Safe stable library uid.
    pub uid: String,
    /// Local address for mpv and ffmpeg.
    pub video: String,
    /// Local address of separate audio, if any.
    pub audio: Option<String>,
    /// Downloaded text subtitle files.
    pub subtitles: Vec<PathBuf>,
    /// Keep the relays alive for all readers of this video.
    pub relays: Arc<Vec<Relay>>,
}

/// Worker service; it performs no network requests on startup.
pub trait WebVideoService: Send {
    /// Resolve this explicitly opened URL off the UI thread.
    fn open(&self, request: Request, done: Box<dyn FnOnce(Result<Ready, String>) + Send>);
}
