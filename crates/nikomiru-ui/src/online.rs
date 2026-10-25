// SPDX-License-Identifier: GPL-3.0-only

//! Online subtitles from the player ([ADR 0008]): the jobs the app's `online`
//! worker runs, the Online subtitles page (MET-009) and the automatic
//! subtitles (MET-007).
//!
//! [ADR 0008]: ../../../docs/adr/0008-online-subtitles.md

use std::path::PathBuf;

use nikomiru_core::config::{NetworkConfig, OnlineConfig};
use nikomiru_online::{Candidate, ProviderId};

/// What to search for: the video, and the title, season and episode (from
/// its file name, or as the user changed them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineQuery {
    /// The video.
    pub video: PathBuf,
    /// The title.
    pub title: String,
    /// The season.
    pub season: Option<u32>,
    /// The episode; `None` for a film.
    pub episode: Option<u32>,
}

impl OnlineQuery {
    /// The query of a video, from its file name (MET-010).
    pub fn of_video(video: PathBuf) -> Self {
        let info = nikomiru_text::filename::parse_path(&video);
        Self {
            video,
            title: info.title,
            season: info.season,
            episode: info.episode,
        }
    }
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OnlineResults {
    /// The files, ranked (ADR 0008 §9).
    pub candidates: Vec<Candidate>,
    /// Why the providers that failed did, translated.
    pub failed: Vec<String>,
}

/// A subtitle file downloaded and kept for a video (MET-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    /// Where it is.
    pub path: PathBuf,
    /// Who gave it.
    pub provider: ProviderId,
    /// Its name at the provider.
    pub file_name: String,
}

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The app's `online` worker. The answers come back on its thread; errors
/// are translated messages.
pub trait OnlineService: Send {
    /// Searches every provider with a key.
    fn search(&self, query: OnlineQuery, done: Done<OnlineResults>);
    /// Downloads one file and keeps it for the video.
    fn download(&self, query: OnlineQuery, candidate: Candidate, done: Done<Downloaded>);
    /// Automatic subtitles: the best file that fits, if any (MET-007).
    fn automatic(&self, query: OnlineQuery, done: Done<Option<Downloaded>>);
    /// New settings.
    fn configure(&self, online: OnlineConfig, network: NetworkConfig);
}

/// No `online` worker: every request is refused with this message.
#[derive(Debug, Default)]
pub struct NoOnline(pub String);

impl OnlineService for NoOnline {
    fn search(&self, _: OnlineQuery, done: Done<OnlineResults>) {
        done(Err(self.0.clone()));
    }

    fn download(&self, _: OnlineQuery, _: Candidate, done: Done<Downloaded>) {
        done(Err(self.0.clone()));
    }

    fn automatic(&self, _: OnlineQuery, done: Done<Option<Downloaded>>) {
        done(Err(self.0.clone()));
    }

    fn configure(&self, _: OnlineConfig, _: NetworkConfig) {}
}

/// A job the session queued; the UI layer starts it on the service and sends
/// the answer back as a message.
#[derive(Debug)]
pub enum OnlineJob {
    /// A search from the page.
    Search {
        /// The video it is for (the session's count of opened videos).
        generation: u64,
        /// What.
        query: OnlineQuery,
    },
    /// A file chosen on the page.
    Download {
        /// The video it is for.
        generation: u64,
        /// The search it comes from.
        query: OnlineQuery,
        /// The file.
        candidate: Box<Candidate>,
    },
    /// Automatic subtitles for the video just opened.
    Automatic {
        /// The video it is for.
        generation: u64,
        /// What.
        query: OnlineQuery,
    },
    /// New settings.
    Configure(Box<(OnlineConfig, NetworkConfig)>),
}

/// The Online subtitles page in the session (MET-009).
#[derive(Debug, Default)]
pub struct OnlinePage {
    /// The search as last filled: from the file name of the video, until the
    /// user changes it.
    pub query: Option<OnlineQuery>,
    /// The last results.
    pub results: Option<OnlineResults>,
    /// The last message, and whether it is good news.
    pub status: Option<(String, bool)>,
    /// A request is running.
    pub busy: bool,
    /// Files loaded from the page: the page closes on each.
    pub loaded: u64,
}
