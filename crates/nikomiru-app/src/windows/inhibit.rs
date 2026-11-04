// SPDX-License-Identifier: GPL-3.0-only

//! The screen kept on during playback (PLY-013), on Windows (ADR 0009):
//! `SetThreadExecutionState`, which holds for the thread that calls it, so
//! one thread of its own makes every call.

use std::sync::mpsc;

use nikomiru_ui::session::IdleInhibitor;
use windows_sys::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
};

/// The playback's wish, carried to the thread that holds it.
#[derive(Debug)]
pub struct PortalInhibitor {
    wishes: mpsc::Sender<bool>,
}

impl PortalInhibitor {
    /// `reason` is for desktops that show one; Windows does not.
    pub fn start(_reason: String) -> Self {
        let (wishes, rx) = mpsc::channel::<bool>();
        let spawned = std::thread::Builder::new()
            .name("inhibit".into())
            .spawn(move || {
                for on in rx {
                    let flags = if on {
                        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
                    } else {
                        ES_CONTINUOUS
                    };
                    // SAFETY: plain flags, for this thread only.
                    if unsafe { SetThreadExecutionState(flags) } == 0 {
                        tracing::warn!("SetThreadExecutionState refused");
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("inhibit thread: {e}");
        }
        Self { wishes }
    }
}

impl IdleInhibitor for PortalInhibitor {
    fn set(&self, inhibit: bool) {
        let _ = self.wishes.send(inhibit);
    }
}
