// SPDX-License-Identifier: GPL-3.0-only

//! Bridge to the Qt Quick video item (C++ in `cpp/`, ADR 0003).

#[cxx_qt::bridge]
mod ffi {
    // SAFETY: the declarations match `cpp/videoitem.h`; both functions take only
    // plain values and are called from the GUI thread.
    unsafe extern "C++" {
        include!("nikomiru-player/videoitem.h");

        fn nikomiru_video_init();
        fn nikomiru_video_set_core(core: usize);
    }
}

/// Selects the OpenGL scene graph and registers `VideoItem` in the QML module
/// `Nikomiru.Video`. Call after `QGuiApplication` and before the first window.
pub fn init() {
    ffi::nikomiru_video_init();
}

/// The core that every `VideoItem` renders. The player MUST outlive the QML
/// engine (shutdown order of SPECIFICATION §5.2.2).
pub fn set_core(player: &crate::Player) {
    ffi::nikomiru_video_set_core(player.raw_handle() as usize);
}
