// SPDX-License-Identifier: GPL-3.0-only

//! The dictionaries page (DIC-001, DIC-010, DIC-016, DIC-030…DIC-034): import,
//! order, enable, rename, popup block state, delete.

use super::View;
use crate::session::Session;
use crate::ui::Command;

/// What the page keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {}

pub(crate) fn show(_ctx: &egui::Context, _s: &Session, _view: &mut View, _out: &mut Vec<Command>) {}
