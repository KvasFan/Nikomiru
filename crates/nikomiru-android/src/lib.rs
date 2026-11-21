// SPDX-License-Identifier: GPL-3.0-only

//! Nikomiru on Android (D-16, ADR 0009): the NativeActivity loads this
//! library and calls `android_main`, which runs the same program as the
//! desktop's `main` in the activity's event loop.

#![deny(unsafe_code)]
#![deny(missing_docs)]

#[cfg(target_os = "android")]
#[path = "../../nikomiru-app/src/main.rs"]
mod program;

/// The activity's entry point: the app's folders and the activity are set
/// up, then the program runs until the activity ends.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
// SAFETY: android-activity calls this one symbol by name, with a Rust
// signature it declares the same way; nothing else is named so.
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    nikomiru_app::platform::android::start(app);
    let code = program::main();
    tracing::info!("android_main ends: {code:?}");
}
