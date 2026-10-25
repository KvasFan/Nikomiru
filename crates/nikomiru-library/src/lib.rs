// SPDX-License-Identifier: GPL-3.0-only

//! Video library: folder import, file name parser, progress and viewing
//! statistics (milestones M5 and M6), and the dictionary registry (M3).
//!
//! Pure logic: no UI toolkit and no `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cards;
pub mod db;
pub mod dictionaries;
pub mod progress;
pub mod uid;
pub mod writer;

pub use cards::MinedCard;
pub use db::{Library, LibraryError, VideoRecord};
pub use dictionaries::{Check, Collapse, DictionaryError, DictionaryManager, DictionaryRecord};
pub use writer::Writer;
