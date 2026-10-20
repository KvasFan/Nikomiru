// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle overlay (SUB-009, SUB-010, SUB-014, POP-001…003): cues over the
//! video, clickable and hoverable per grapheme.

use crate::session::Session;
use crate::ui::Command;

/// What the overlay keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {}

pub(crate) fn show(
    _ctx: &egui::Context,
    _s: &Session,
    _state: &mut State,
    _bar_height: f32,
    _out: &mut Vec<Command>,
) {
}
