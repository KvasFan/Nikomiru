// SPDX-License-Identifier: GPL-3.0-only

//! The parts of the Nikomiru application that do not need Qt's event loop:
//! command line, single instance, logs, signals and the desktop portal. The
//! binary (`src/main.rs`) wires them to Qt; keeping them in a library lets the
//! integration tests in `tests/` use them.
//!
//! `unsafe` is allowed only in the `linux` module (environment, `setlocale`,
//! signals) and in the cxx-qt bridge modules (ARC-011).

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod cli;
pub mod dbus;
pub mod inhibit;
pub mod instance;
#[allow(unsafe_code)]
pub mod linux;
pub mod logging;
