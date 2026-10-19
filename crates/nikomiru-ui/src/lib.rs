// SPDX-License-Identifier: GPL-3.0-only

//! The QML interface and its Rust `QObject`s (cxx-qt): player, subtitle overlay,
//! dictionary popup, library and settings (D-10).
//!
//! `unsafe` is denied; only the cxx-qt bridge modules (`src/bridge.rs` or
//! `src/bridge/`), which hold generated FFI declarations, may allow it (ARC-011).

#![deny(unsafe_code)]
#![deny(missing_docs)]

#[allow(unsafe_code, missing_docs, clippy::too_many_arguments)]
mod bridge {
    pub mod player;
}
pub mod controls;
pub mod dictionaries;
#[cfg(feature = "test_harness")]
pub mod harness;
pub mod i18n;
pub mod popup;
pub mod popup_view;
pub mod screensaver;
pub mod session;
pub mod shortcuts;
pub mod subtitles;

pub use bridge::player::{deliver, install, request_activate, request_open, request_quit};
pub use session::Services;
