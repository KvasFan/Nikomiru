// SPDX-License-Identifier: GPL-3.0-only

//! The library from the player (LIB-002…LIB-012): the jobs the app's
//! `library` worker runs off the UI thread (scans, covers, the reads of the
//! home page, the user's edits) and the Library page.

use std::collections::BTreeMap;
use std::path::PathBuf;

use nikomiru_library::home::SeriesSummary;
use nikomiru_library::import::ImportReport;
use nikomiru_library::series_info::SeriesInfoRecord;
use nikomiru_library::stats::{DayTotal, Segment};
use nikomiru_library::{FolderRecord, Grouping, VideoRecord};

/// What the home page shows (LIB-005).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Home {
    /// "Continue watching": the last played first.
    pub continue_watching: Vec<(String, VideoRecord)>,
    /// The series, by name.
    pub series: Vec<SeriesSummary>,
    /// The imported folders, by path.
    pub folders: Vec<(String, FolderRecord)>,
}

impl Home {
    /// The title of a series on the page.
    pub fn series_title(&self, id: u64) -> Option<&str> {
        self.series
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.record.title.as_str())
    }
}

/// What the statistics page shows (STA-005).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stats {
    /// The totals of each day studied, by day.
    pub days: Vec<DayTotal>,
    /// Time and characters by series (or file), the most studied first.
    pub series: Vec<(String, i64, u64)>,
    /// The latest sessions, with the name of their video.
    pub sessions: Vec<(u128, Segment, String)>,
}

/// The sessions the page lists.
pub const SESSIONS_SHOWN: usize = 200;

/// A cover decoded off the UI thread.
#[derive(Clone, PartialEq, Eq)]
pub struct Cover {
    /// Width and height.
    pub size: [usize; 2],
    /// The pixels, RGBA.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Cover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cover").field("size", &self.size).finish()
    }
}

impl Cover {
    /// Decodes a JPEG or PNG cover.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        // the limits of the dictionaries' images: a cover may come from the
        // network (MET-001)
        let pixels = crate::popup_view::decode_image(bytes)
            .ok_or_else(|| "not an image Nikomiru reads, or too large".to_string())?;
        Ok(Self {
            size: pixels.size,
            rgba: pixels.rgba,
        })
    }
}

/// An edit of the library by the user (LIB-011, LIB-012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryEdit {
    /// A series renamed.
    Rename(u64, String),
    /// A series deleted from the library; its files stay.
    Delete(u64),
    /// A series merged into another.
    Merge {
        /// The one merged.
        from: u64,
        /// The one it goes into.
        into: u64,
    },
    /// Episodes moved to another series.
    Move {
        /// The episodes.
        uids: Vec<String>,
        /// Where.
        to: u64,
    },
    /// Episodes split off into a new series.
    Split {
        /// The episodes.
        uids: Vec<String>,
        /// The new series' name.
        title: String,
    },
    /// Episodes removed from the library; their files stay.
    Remove(Vec<String>),
    /// A study session deleted (STA-005).
    DeleteSession(u128),
    /// What was found online of a series (MET-001), or `None` to forget it;
    /// its cover, already downloaded, becomes the series' own.
    Identify {
        /// The series.
        id: u64,
        /// What was found.
        info: Option<Box<SeriesInfoRecord>>,
        /// The cover image.
        cover: Option<Vec<u8>>,
    },
}

/// A series opened: its episodes in order, and what is known of it online.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesDetail {
    /// The episodes.
    pub episodes: Episodes,
    /// What AniList or Jikan said of it (MET-001).
    pub info: Option<SeriesInfoRecord>,
}

/// A series being identified online (MET-001, MET-002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identify {
    /// The series.
    pub series: u64,
    /// The source asked.
    pub source: Option<nikomiru_online::metadata::Source>,
    /// What to choose from, when no result has exactly its title.
    pub choices: Vec<nikomiru_online::metadata::SeriesInfo>,
    /// A request is running.
    pub busy: bool,
}

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The library worker (the app's `library` thread).
pub trait LibraryService: Send {
    /// The home page.
    fn home(&self, done: Done<Home>);
    /// The episodes of a series, in order, and what is known of it online.
    fn episodes(&self, series: u64, done: Done<SeriesDetail>);
    /// Imports `root`, or scans it again, with `grouping`.
    fn import(&self, root: PathBuf, grouping: Grouping, done: Done<ImportReport>);
    /// Applies an edit.
    fn edit(&self, edit: LibraryEdit, done: Done<()>);
    /// The cover of `series`: the one kept, or the frame of its first
    /// episode, made and kept.
    fn cover(&self, series: u64, done: Done<Cover>);
    /// The statistics (STA-005).
    fn stats(&self, done: Done<Stats>);
    /// The sessions as CSV, written to `path` (STA-005).
    fn export_csv(&self, path: PathBuf, done: Done<PathBuf>);
}

/// No library (it could not be opened): every job fails with the reason.
#[derive(Debug, Default)]
pub struct NoLibrary(pub String);

impl LibraryService for NoLibrary {
    fn home(&self, done: Done<Home>) {
        done(Err(self.0.clone()));
    }

    fn episodes(&self, _: u64, done: Done<SeriesDetail>) {
        done(Err(self.0.clone()));
    }

    fn import(&self, _: PathBuf, _: Grouping, done: Done<ImportReport>) {
        done(Err(self.0.clone()));
    }

    fn edit(&self, _: LibraryEdit, done: Done<()>) {
        done(Err(self.0.clone()));
    }

    fn cover(&self, _: u64, done: Done<Cover>) {
        done(Err(self.0.clone()));
    }

    fn stats(&self, done: Done<Stats>) {
        done(Err(self.0.clone()));
    }

    fn export_csv(&self, _: PathBuf, done: Done<PathBuf>) {
        done(Err(self.0.clone()));
    }
}

/// A job for the library worker, taken from the session by the UI layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryJob {
    /// Read the home page.
    Home,
    /// Read the episodes of a series.
    Episodes(u64),
    /// Import or scan a folder.
    Import(PathBuf, Grouping),
    /// Apply an edit.
    Edit(LibraryEdit),
    /// Make or read the cover of a series.
    Cover(u64),
    /// Read the statistics.
    Stats,
    /// Write the sessions as CSV.
    ExportCsv(PathBuf),
}

/// What the library worker answers, back on the UI thread.
#[derive(Debug)]
pub enum LibraryReply {
    /// The home page.
    Home(Result<Home, String>),
    /// The episodes of a series, and what is known of it online.
    Episodes(u64, Result<SeriesDetail, String>),
    /// A folder imported or scanned.
    Imported(Result<ImportReport, String>),
    /// An edit applied.
    Edited(Result<(), String>),
    /// The cover of a series.
    Cover(u64, Result<Cover, String>),
    /// The statistics.
    Stats(Result<Stats, String>),
    /// The CSV written, or not.
    Exported(Result<PathBuf, String>),
}

/// Episodes by uid, in order.
pub type Episodes = Vec<(String, VideoRecord)>;

/// The Library page in the session.
#[derive(Debug, Default)]
pub struct LibraryPage {
    /// The home page, once read.
    pub home: Option<Home>,
    /// The series open, and its episodes once read.
    pub series: Option<(u64, Option<Episodes>)>,
    /// The covers decoded, by series.
    pub covers: BTreeMap<u64, Cover>,
    /// The covers asked for, made or not.
    pub covers_asked: Vec<u64>,
    /// A scan or an edit is running.
    pub busy: bool,
    /// The last message, and whether it is good news.
    pub status: Option<(String, bool)>,
    /// The statistics, once read (STA-005).
    pub stats: Option<Stats>,
    /// What is known online of the series open (MET-001).
    pub series_info: Option<SeriesInfoRecord>,
    /// The series being identified online.
    pub identify: Option<Identify>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(width, height)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    // a cover may come from the network (MET-001): it is decoded within limits
    #[test]
    fn covers_are_decoded_within_limits() {
        let cover = Cover::decode(&png(320, 180)).unwrap();
        assert_eq!(cover.size, [320, 180]);
        assert_eq!(cover.rgba.len(), 320 * 180 * 4);
        // wider than any screen: refused, not allocated
        assert!(Cover::decode(&png(9000, 1)).is_err());
        assert!(Cover::decode(b"<html>not an image</html>").is_err());
    }
}
