// SPDX-License-Identifier: GPL-3.0-only

//! Video library: folder import, file name parser, progress and viewing
//! statistics (milestones M5 and M6).
//!
//! Pure logic: no `dioxus`, `gtk` or `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod db;
pub mod progress;
pub mod uid;

pub use db::{Library, LibraryError, VideoRecord};
