// SPDX-License-Identifier: GPL-3.0-only

//! The screensaver kept off during playback (PLY-013), on Windows: not yet;
//! `SetThreadExecutionState` would do it (spike S6).

use nikomiru_ui::session::IdleInhibitor;

/// Says once that it does nothing.
#[derive(Debug)]
pub struct PortalInhibitor;

impl PortalInhibitor {
    /// `reason` is for the desktop, which is not asked.
    pub fn start(_reason: String) -> Self {
        tracing::info!("the screensaver is not held off on Windows yet");
        Self
    }
}

impl IdleInhibitor for PortalInhibitor {
    fn set(&self, _inhibit: bool) {}
}
