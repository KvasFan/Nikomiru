// SPDX-License-Identifier: GPL-3.0-only

//! Text processing: normalization, kana conversion, preprocessors, sentence
//! extraction, furigana and character counting (milestone M3), and the
//! title, season and episode of a file name (MET-010, LIB-003).
//!
//! Pure logic: no UI toolkit and no `libmpv2` (ARC-010). It knows nothing about video,
//! so books and manga can use it after 1.0 (D-12).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod filename;
pub mod furigana;
pub mod hangul;
pub mod kana;
mod kanji_variants;
pub mod scan;
pub mod sentence;
pub mod variants;
