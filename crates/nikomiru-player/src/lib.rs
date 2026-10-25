// SPDX-License-Identifier: GPL-3.0-only

//! The mpv core (libmpv2): initialization, observed properties, commands and
//! events; plus the render context that draws the video into the window
//! (milestone M1, ADR 0006).
//!
//! It is the only workspace crate that depends on `libmpv2` (ARC-010). `unsafe` is
//! allowed here for the mpv FFI, always with a `// SAFETY:` comment (ARC-011).

#![deny(missing_docs)]

pub mod control;
pub mod options;
pub mod player;
pub mod render;
pub mod settings;
pub mod word;

pub use player::{
    CommandReply, LoadRequest, PlaybackError, Player, PlayerError, PlayerEvent, PlayerHandle,
    VideoOutput,
};
