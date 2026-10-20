// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle list (SUB-015): every cue with its time, the current one
//! highlighted, search, a click on the time seeks and a click on the text looks
//! the word up.

use crate::session::Session;
use crate::ui::Command;

/// What the list keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {}

pub(crate) fn show(
    _ctx: &egui::Context,
    _s: &Session,
    _state: &mut State,
    _search_focus: &mut bool,
    _bar_height: f32,
    _out: &mut Vec<Command>,
) {
}
