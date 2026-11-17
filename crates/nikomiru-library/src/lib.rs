// SPDX-License-Identifier: GPL-3.0-only

//! Video library: folder import and series (M5), progress and viewing
//! statistics (M5, M6), mined cards (M4) and the dictionary registry (M3).
//!
//! Pure logic: no UI toolkit and no `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Reader bookmarks, highlights and notes.
pub mod annotations;
pub mod backup;
pub mod batches;
pub mod books;
pub mod cards;
pub mod cloze;
pub mod db;
pub mod dictionaries;
pub mod encounters;
pub mod grouping;
pub mod home;
pub mod import;
/// Independent local phrase vocabulary.
pub mod phrases;
/// Podcast feeds and episodes.
pub mod podcasts;
pub mod policy;
pub mod progress;
pub mod review;
pub mod review_log;
pub mod scan;
pub mod sentences;
pub mod series_info;
pub mod server_items;
pub mod stats;
pub mod sync;
pub mod uid;
pub mod words;
pub mod writer;

pub use cards::MinedCard;
pub use db::{
    FolderRecord, Grouping, Library, LibraryError, SeriesPrefs, SeriesRecord, SubtitleChoice,
    VideoRecord,
};
pub use dictionaries::{Check, Collapse, DictionaryError, DictionaryManager, DictionaryRecord};
pub use writer::Writer;
