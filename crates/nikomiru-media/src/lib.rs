// SPDX-License-Identifier: GPL-3.0-only

//! External ffmpeg and ffprobe processes with timeouts and cancellation: extraction
//! of embedded subtitles, sentence audio and images (milestones M2 and M4), audio
//! levels (M4b) and the covers of the library (M5).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod clip;
pub mod cover;
pub mod energy;
pub mod extract;
pub mod process;
pub mod tools;
