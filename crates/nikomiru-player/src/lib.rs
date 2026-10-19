// SPDX-License-Identifier: GPL-3.0-only

//! The mpv core (libmpv2): initialization, observed properties, commands and
//! events; plus the Qt Quick video item (C++ in `cpp/`, rendered on the scene
//! graph's render thread) and its bridge (milestone M1, ADR 0003).
//!
//! It is the only workspace crate that depends on `libmpv2` (ARC-010) and the only
//! one with C++ code. `unsafe` is allowed here for the mpv FFI and the bridge to the
//! video item, always with a `// SAFETY:` comment (ARC-011).

#![deny(missing_docs)]

mod bridge;
pub mod control;
pub mod options;
pub mod player;

/// The Qt Quick video item (C++ in `cpp/`, ADR 0003).
pub mod video {
    pub use crate::bridge::{init, set_core};
}

pub use player::{
    LoadRequest, PlaybackError, Player, PlayerError, PlayerEvent, PlayerHandle, VideoOutput,
};
