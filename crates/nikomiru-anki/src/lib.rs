// SPDX-License-Identifier: GPL-3.0-only

//! Mining to Anki: the AnkiConnect client, the field composer, the Lapis note
//! type and the `nikomiru://` links back to the scene (milestone M4,
//! [ADR 0007](../../../docs/adr/0007-mining-to-anki.md)).
//!
//! Pure logic that knows nothing about video (D-12): no egui, winit or libmpv2
//! (ARC-010). The `anki` worker of the app runs the mining of a card.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod base64;
pub mod client;
pub mod error;
#[cfg(feature = "test-support")]
pub mod fake;
pub mod lapis;
pub mod media;

pub use client::{Client, DuplicateScope, NewModel, Note, NoteInfo};
pub use error::AnkiError;
