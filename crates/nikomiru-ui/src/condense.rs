// SPDX-License-Identifier: GPL-3.0-only

//! Condensed audio requests, destination choice and cancellation (AUD-010).

use nikomiru_core::{Cancel, Config};
use nikomiru_media::condense::{Format, Line};
use std::path::PathBuf;

/// What the user asked to export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The currently loaded subtitles, offset and audio track.
    Current,
    /// A library episode, with its saved choices.
    Episode(String),
    /// A series in library order, one output per episode.
    Series(u64),
}

/// The current player's choices, captured before the destination dialog.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Audio input (possibly an external audio track).
    pub input: PathBuf,
    /// Audio stream ff-index.
    pub stream: Option<i64>,
    /// Padded main subtitle lines, with the offset already counted.
    pub lines: Vec<Line>,
    /// Media duration.
    pub duration_ms: i64,
}

/// Work sent to the library thread.
#[derive(Debug, Clone)]
pub struct Request {
    /// The requested episode or series.
    pub target: Target,
    /// Current-episode snapshot, when requested.
    pub current: Option<Snapshot>,
    /// Destination folder.
    pub folder: PathBuf,
    /// Output format.
    pub format: Format,
    /// Settings at the time of the request.
    pub config: Box<Config>,
    /// Where downloaded subtitles are kept.
    pub subtitles_dir: PathBuf,
    /// Stops this export, without cancelling other library work.
    pub cancel: Cancel,
}

/// The series may have successful and failed episodes.
#[derive(Debug, Default)]
pub struct Report {
    /// Complete files, never partial outputs.
    pub written: Vec<PathBuf>,
    /// Episode names and their errors.
    pub failed: Vec<(String, String)>,
}

/// Destination dialog and running export; only one export runs at a time.
#[derive(Debug, Default)]
pub struct State {
    /// A request waiting for its destination folder.
    pub pending: Option<Request>,
    /// Cancellation token of the running export.
    pub running: Option<Cancel>,
    /// Last completed report.
    pub report: Option<Report>,
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(cancel) = &self.running {
            cancel.cancel();
        }
    }
}
