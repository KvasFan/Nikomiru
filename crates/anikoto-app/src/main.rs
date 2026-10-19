// SPDX-License-Identifier: GPL-3.0-only

//! The Anikoto binary: Dioxus startup, GTK + mpv compositor and the wiring between
//! the crates.
//!
//! `unsafe` is allowed only in the `linux` module (GTK/EGL and iconv FFI, ARC-011).

#![deny(unsafe_code)]

fn main() {
    println!("Anikoto {}", env!("CARGO_PKG_VERSION"));
}
