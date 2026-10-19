// SPDX-License-Identifier: GPL-3.0-only

//! The Nikomiru binary: `QGuiApplication`, the QML engine, single instance and the
//! wiring between the crates (D-10).
//!
//! `unsafe` is allowed only in the `linux` module (iconv, setlocale) and in the
//! cxx-qt bridge modules (ARC-011).

#![deny(unsafe_code)]

fn main() {
    println!("Nikomiru {}", env!("CARGO_PKG_VERSION"));
}
