// SPDX-License-Identifier: GPL-3.0-only

//! File dialogs through the XDG desktop portal
//! (`org.freedesktop.portal.FileChooser`), as Qt's dialogs did: they work in a
//! Flatpak sandbox too (D-5).

use std::sync::Arc;

use nikomiru_ui::{FileDialog, Waker};

/// Opens `dialog` on a thread; the chosen file comes back to the interface as
/// `Inbound::Chosen`.
pub fn open_file(dialog: FileDialog, waker: Arc<dyn Waker>) {
    tracing::warn!("file dialog {:?}: not available yet", dialog.dialog);
    drop(waker);
}
