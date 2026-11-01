// SPDX-License-Identifier: GPL-3.0-only

//! Subtitles: SRT, WebVTT and ASS parsers, cue index, offsets and encoding
//! detection (milestone M2). mpv draws no text subtitle: Nikomiru parses them
//! into cues and draws them itself (SPECIFICATION §6.4).
//!
//! Pure logic: no UI toolkit and no `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod align;
pub mod ass;
pub mod cue;
pub mod effects;
pub mod encoding;
pub mod lrc;
mod markup;
pub mod select;
pub mod srt;
mod time;
pub mod timeline;
pub mod vtt;

pub use cue::{
    AssStyle, Cue, Format, Grapheme, Markup, Rgb, Ruby, Run, RunStyle, TextAlign, Track,
};

/// Parses `text` as `format`.
pub fn parse(format: Format, text: &str) -> Track {
    match format {
        Format::Srt => srt::parse(text),
        Format::Vtt => vtt::parse(text),
        Format::Ass => ass::parse(text),
        Format::Lrc => lrc::parse(text),
    }
}
