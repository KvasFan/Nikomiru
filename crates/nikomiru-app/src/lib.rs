// SPDX-License-Identifier: GPL-3.0-only

//! The Nikomiru application: command line, single instance, logs, signals, the
//! desktop portal, the dictionary and subtitle workers, and the window with its
//! event loop (egui, D-13). The binary (`src/main.rs`) wires them together;
//! keeping them in a library lets the integration tests in `tests/` use them.
//!
//! `unsafe` is allowed only in the `platform` module (`linux/` on Linux:
//! environment, `setlocale`, signals, iconv) and in the `window` module (the OpenGL context and mpv's
//! render context), ARC-011.

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod anki;
#[cfg(target_os = "linux")]
pub mod appearance;
pub mod catalog;
pub mod cli;
#[cfg(target_os = "linux")]
pub mod dbus;
pub mod dictionaries;
#[cfg(feature = "test-harness")]
pub mod harness;
pub mod hooker;
#[cfg_attr(windows, path = "windows/inhibit.rs")]
#[cfg_attr(windows, allow(unsafe_code))]
pub mod inhibit;
#[cfg_attr(windows, path = "windows/instance.rs")]
#[cfg_attr(windows, allow(unsafe_code))]
pub mod instance;
#[cfg_attr(windows, path = "windows/keyring.rs")]
#[cfg_attr(windows, allow(unsafe_code))]
pub mod keyring;
pub mod logging;
pub mod ocr;
pub mod online;
#[allow(unsafe_code)]
#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(windows, path = "windows/mod.rs")]
pub mod platform;
#[cfg_attr(windows, path = "windows/portal.rs")]
#[cfg_attr(windows, allow(unsafe_code))]
pub mod portal;
pub mod servers;
pub mod shell;
pub mod subtitles;
pub mod sync;
pub mod thumbnails;
#[allow(unsafe_code)]
pub mod window;
pub mod word_audio;
