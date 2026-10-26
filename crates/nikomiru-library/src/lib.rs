// SPDX-License-Identifier: GPL-3.0-only

//! Video library: folder import and series (M5), progress and viewing
//! statistics (M5, M6), mined cards (M4) and the dictionary registry (M3).
//!
//! Pure logic: no UI toolkit and no `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cards;
pub mod db;
pub mod dictionaries;
pub mod grouping;
pub mod home;
pub mod import;
pub mod policy;
pub mod progress;
pub mod scan;
pub mod series_info;
pub mod stats;
pub mod uid;
pub mod writer;

pub use cards::MinedCard;
pub use db::{
    FolderRecord, Grouping, Library, LibraryError, SeriesPrefs, SeriesRecord, SubtitleChoice,
    VideoRecord,
};
pub use dictionaries::{Check, Collapse, DictionaryError, DictionaryManager, DictionaryRecord};
pub use writer::Writer;
