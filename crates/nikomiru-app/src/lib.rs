// SPDX-License-Identifier: GPL-3.0-only

//! The Nikomiru application: command line, single instance, logs, signals, the
//! desktop portal, the dictionary and subtitle workers, and the window with its
//! event loop (egui, D-13). The binary (`src/main.rs`) wires them together;
//! keeping them in a library lets the integration tests in `tests/` use them.
//!
//! `unsafe` is allowed only in the `linux` module (environment, `setlocale`,
//! signals, iconv) and in the `window` module (the OpenGL context and mpv's
//! render context), ARC-011.

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod anki;
pub mod appearance;
pub mod catalog;
pub mod cli;
pub mod dbus;
pub mod dictionaries;
#[cfg(feature = "test-harness")]
pub mod harness;
pub mod inhibit;
pub mod instance;
#[allow(unsafe_code)]
pub mod linux;
pub mod logging;
pub mod online;
pub mod portal;
pub mod shell;
pub mod subtitles;
pub mod thumbnails;
#[allow(unsafe_code)]
pub mod window;
pub mod word_audio;
