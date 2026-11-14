// SPDX-License-Identifier: GPL-3.0-only

//! The interface of Nikomiru, drawn with egui (D-13, ADR 0006): the session,
//! which turns player events, keys and clicks into state and commands, and the
//! view, which draws that state every frame and turns clicks into commands.
//!
//! The app drives it from its event loop ([`Ui`]); the work done on other
//! threads comes back as [`Inbound`] messages.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod book_scores;
pub mod books;
pub mod catalog;
pub mod condense;
pub mod controls;
pub mod dictionaries;
mod encounters;
mod gaps;
#[cfg(feature = "test-harness")]
pub mod harness;
pub mod hooker;
pub mod hover;
pub mod i18n;
pub mod mining;
pub mod ocr;
pub mod online;
pub mod player_settings;
pub mod popup;
pub mod popup_view;
pub mod reader_words;
pub mod review;
pub mod scores;
pub mod screensaver;
pub mod screenshot;
pub mod series_scores;
pub mod servers;
pub mod session;
pub mod setup;
pub mod shortcuts;
pub mod study;
pub mod subtitles;
pub mod sync;
pub mod thumbnails;
mod ui;
pub mod view;
pub mod web_video;
pub mod word_audio;
pub mod word_export;
pub mod words;

pub use session::Services;
pub use shortcuts::Modifiers;
pub use ui::{Inbound, Key, Ui, Waker, Wheel, is_web_address};
pub use view::{Dialog, FileDialog};

/// Bounded plain SRT export of complete current subtitles.
pub mod subtitle_export;
pub mod subtitle_study;

/// Local snapshots of completed comprehension summaries.
pub mod study_report;
