// SPDX-License-Identifier: GPL-3.0-only

//! File dialogs and links (APP-003, POP-011), on Windows: not yet; the common
//! item dialog and `ShellExecuteW` would do them (spike S6). Files open from
//! the command line or by a drop.

use std::sync::Arc;

use nikomiru_ui::{FileDialog, Inbound, Waker};

/// Says the dialog failed, as a desktop without a portal does.
pub fn open_file(_dialog: FileDialog, waker: Arc<dyn Waker>) {
    tracing::warn!("no file dialog on Windows yet");
    waker.wake(Inbound::DialogFailed);
}

/// Not opened.
pub fn open_uri(_url: &str) {
    tracing::warn!("links are not opened on Windows yet");
}
