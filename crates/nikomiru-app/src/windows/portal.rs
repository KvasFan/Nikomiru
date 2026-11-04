// SPDX-License-Identifier: GPL-3.0-only

//! File dialogs and links (APP-003, POP-011), on Windows (ADR 0009): the
//! common dialogs of Windows for files, its folder browser for folders, and
//! `ShellExecuteW` for web addresses.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nikomiru_ui::{FileDialog, Inbound, Waker, is_web_address};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Shell::{
    BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW,
    SHGetPathFromIDListW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::platform::{from_wide, wide};

/// One dialog at a time.
static OPEN: AtomicBool = AtomicBool::new(false);

/// Room for a long path, as the dialogs allow.
const PATH_UNITS: usize = 32_768;

/// Opens `dialog` on a thread; the chosen file comes back to the interface as
/// `Inbound::Chosen`, a dialog that fails as `Inbound::DialogFailed`.
pub fn open_file(dialog: FileDialog, waker: Arc<dyn Waker>) {
    if OPEN.swap(true, Ordering::SeqCst) {
        tracing::debug!("a file dialog is open already");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("nikomiru-file-dialog".into())
        .spawn(move || {
            let result = if dialog.directory {
                choose_folder(&dialog)
            } else {
                choose_file(&dialog)
            };
            OPEN.store(false, Ordering::SeqCst);
            match result {
                Ok(Some(path)) => waker.wake(Inbound::Chosen {
                    dialog: dialog.dialog,
                    path,
                }),
                Ok(None) => tracing::debug!("file dialog cancelled"),
                Err(e) => {
                    tracing::warn!("file dialog: {e}");
                    waker.wake(Inbound::DialogFailed);
                }
            }
        });
    if let Err(e) = spawned {
        OPEN.store(false, Ordering::SeqCst);
        tracing::warn!("file dialog thread: {e}");
    }
}

/// The filters as the common dialogs take them: name, NUL, patterns joined
/// by `;`, NUL, and a NUL at the end.
fn filters(dialog: &FileDialog) -> Vec<u16> {
    let mut text = String::new();
    for (name, patterns) in &dialog.filters {
        text.push_str(name);
        text.push('\0');
        text.push_str(&patterns.join(";"));
        text.push('\0');
    }
    text.push('\0');
    text.encode_utf16().collect()
}

fn choose_file(dialog: &FileDialog) -> Result<Option<PathBuf>, String> {
    let title = wide(&dialog.title);
    let filter = filters(dialog);
    let folder = dialog.folder.as_ref().map(|f| wide(&f.to_string_lossy()));
    let mut file = vec![0u16; PATH_UNITS];
    if let Some(name) = &dialog.save_as {
        let name: Vec<u16> = name.encode_utf16().take(PATH_UNITS - 1).collect();
        file[..name.len()].copy_from_slice(&name);
    }
    let save = dialog.save_as.is_some();
    let flags = OFN_EXPLORER
        | OFN_NOCHANGEDIR
        | OFN_PATHMUSTEXIST
        | if save {
            OFN_OVERWRITEPROMPT
        } else {
            OFN_FILEMUSTEXIST
        };
    let mut options = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: if dialog.filters.is_empty() {
            std::ptr::null()
        } else {
            filter.as_ptr()
        },
        nFilterIndex: 1,
        lpstrFile: file.as_mut_ptr(),
        nMaxFile: PATH_UNITS as u32,
        lpstrInitialDir: folder.as_ref().map_or(std::ptr::null(), |f| f.as_ptr()),
        lpstrTitle: title.as_ptr(),
        Flags: flags,
        // SAFETY: all-zero is valid for the other fields (no owner window,
        // hook, template or custom filter).
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: every pointer in `options` points into a buffer of this
    // function, alive until the dialog closes; `file` has `nMaxFile` units.
    let chosen = unsafe {
        if save {
            GetSaveFileNameW(&mut options)
        } else {
            GetOpenFileNameW(&mut options)
        }
    };
    if chosen != 0 {
        return Ok(Some(PathBuf::from(from_wide(&file))));
    }
    // SAFETY: reads the dialogs' last error, nothing else.
    match unsafe { CommDlgExtendedError() } {
        0 => Ok(None),
        code => Err(format!("common dialog error {code:#x}")),
    }
}

fn choose_folder(dialog: &FileDialog) -> Result<Option<PathBuf>, String> {
    // SAFETY: the folder browser needs COM on this thread, which is ours and
    // ends with the dialog.
    unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    let title = wide(&dialog.title);
    let mut name = vec![0u16; 260];
    let browse = BROWSEINFOW {
        pszDisplayName: name.as_mut_ptr(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        // SAFETY: all-zero is valid for the other fields (no owner window,
        // root, callback).
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: the strings live until the dialog closes; `name` has the
    // MAX_PATH units the browser writes at most.
    let item = unsafe { SHBrowseForFolderW(&browse) };
    if item.is_null() {
        return Ok(None);
    }
    let mut path = vec![0u16; PATH_UNITS];
    // SAFETY: `item` is the browser's answer, freed once below; `path` has
    // room for any path.
    let found = unsafe { SHGetPathFromIDListW(item, path.as_mut_ptr()) };
    // SAFETY: the item list was allocated by the shell for the caller.
    unsafe { CoTaskMemFree(item.cast()) };
    if found == 0 {
        return Err("the folder chosen has no path".into());
    }
    Ok(Some(PathBuf::from(from_wide(&path))))
}

/// Opens a web address from a dictionary in the browser (POP-011). Anything
/// but `http(s)` is refused again here.
pub fn open_uri(url: &str) {
    if !is_web_address(url) {
        tracing::warn!("not a web address, not opened");
        return;
    }
    let verb = wide("open");
    let target = wide(url);
    // SAFETY: the strings are NUL-terminated and live through the call; no
    // window, parameters or folder.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // values up to 32 are errors
    if result as usize <= 32 {
        tracing::warn!("ShellExecute: error {}", result as usize);
    }
}
