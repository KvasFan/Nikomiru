// SPDX-License-Identifier: GPL-3.0-only

//! The interface of Nikomiru, drawn with egui (D-13, ADR 0006): the session,
//! which turns player events, keys and clicks into state and commands, and the
//! view, which draws that state every frame and turns clicks into commands.
//!
//! The app drives it from its event loop ([`Ui`]); the work done on other
//! threads comes back as [`Inbound`] messages.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod controls;
pub mod dictionaries;
#[cfg(feature = "test-harness")]
pub mod harness;
pub mod hover;
pub mod i18n;
pub mod mining;
pub mod online;
pub mod player_settings;
pub mod popup;
pub mod popup_view;
pub mod screensaver;
pub mod screenshot;
pub mod session;
pub mod shortcuts;
pub mod study;
pub mod subtitles;
mod ui;
pub mod view;
pub mod word_audio;

pub use session::Services;
pub use shortcuts::Modifiers;
pub use ui::{Inbound, Key, Ui, Waker, Wheel, is_web_address};
pub use view::{Dialog, FileDialog};
