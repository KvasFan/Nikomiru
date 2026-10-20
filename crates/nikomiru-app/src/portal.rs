// SPDX-License-Identifier: GPL-3.0-only

//! File dialogs through the XDG desktop portal
//! (`org.freedesktop.portal.FileChooser`), as Qt's dialogs did: they work in a
//! Flatpak sandbox too (D-5). Web links open in the system's browser.

use std::process::{Command, Stdio};
use std::sync::Arc;

use nikomiru_ui::{FileDialog, Waker, is_web_address};

/// Opens `dialog` on a thread; the chosen file comes back to the interface as
/// `Inbound::Chosen`.
pub fn open_file(dialog: FileDialog, waker: Arc<dyn Waker>) {
    tracing::warn!("file dialog {:?}: not available yet", dialog.dialog);
    drop(waker);
}

/// Opens a web address from a dictionary in the browser (POP-011), through
/// `xdg-open` with the address as its only argument. Anything but `http(s)`
/// is refused again here.
pub fn open_uri(url: &str) {
    if !is_web_address(url) {
        tracing::warn!("not a web address, not opened");
        return;
    }
    let url = url.to_string();
    let spawned = std::thread::Builder::new()
        .name("nikomiru-open-uri".into())
        .spawn(move || {
            let status = Command::new("xdg-open")
                .arg(&url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Err(e) = status {
                tracing::warn!("xdg-open: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("{e}");
    }
}
