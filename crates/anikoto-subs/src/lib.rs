// SPDX-License-Identifier: GPL-3.0-only

//! Subtitles: SRT, WebVTT and ASS parsers, cue index, offsets and encoding
//! detection (milestone M2).
//!
//! Pure logic: no `dioxus`, `gtk` or `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
