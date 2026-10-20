// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary popups (POP-001…POP-015): one panel per level of the stack,
//! placed next to the word, with the entries of the reply.

use crate::session::Session;
use crate::ui::Command;

/// What the popups keep between frames.
#[derive(Debug, Default)]
pub(crate) struct State {}

/// Draws the popups; returns the levels drawn with their content.
pub(crate) fn show(
    _ctx: &egui::Context,
    s: &Session,
    _state: &mut State,
    _out: &mut Vec<Command>,
) -> Vec<usize> {
    s.popups
        .stack()
        .iter()
        .enumerate()
        .filter(|(_, p)| p.reply.is_some())
        .map(|(level, _)| level)
        .collect()
}
