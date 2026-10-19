// SPDX-License-Identifier: GPL-3.0-only

//! The QML interface and its Rust `QObject`s (cxx-qt): player, subtitle overlay,
//! dictionary popup, library and settings (D-10).
//!
//! `unsafe` is denied; only the cxx-qt bridge modules (`src/bridge.rs` or
//! `src/bridge/`), which hold generated FFI declarations, may allow it (ARC-011).

#![deny(unsafe_code)]
#![deny(missing_docs)]

#[allow(unsafe_code, missing_docs)]
mod bridge {
    pub mod player;
}
pub mod controls;
pub mod i18n;
pub mod session;
pub mod shortcuts;

pub use bridge::player::{deliver, install, request_open};
pub use session::Services;
