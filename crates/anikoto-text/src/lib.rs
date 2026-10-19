// SPDX-License-Identifier: GPL-3.0-only

//! Text processing: normalization, kana conversion, preprocessors, sentence
//! extraction, furigana and character counting (milestone M3).
//!
//! Pure logic: no `dioxus`, `gtk` or `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
