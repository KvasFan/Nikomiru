// SPDX-License-Identifier: GPL-3.0-only

//! Text processing: normalization, kana conversion, preprocessors, sentence
//! extraction, furigana and character counting (milestone M3).
//!
//! Pure logic: no Qt and no `libmpv2` (ARC-010). It knows nothing about video,
//! so books and manga can use it after 1.0 (D-12).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod furigana;
pub mod hangul;
pub mod kana;
mod kanji_variants;
pub mod scan;
pub mod sentence;
pub mod variants;
