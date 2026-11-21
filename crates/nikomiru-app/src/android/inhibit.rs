// SPDX-License-Identifier: GPL-3.0-only

//! The screen kept on during playback (PLY-013), on Android (D-16): the
//! window's `FLAG_KEEP_SCREEN_ON`, set by Java's main thread through JNI.
//! (android-activity's own `set_window_flags` takes a lock the event loop
//! holds while it waits for events: called while the loop runs, it stalls
//! the app.)

use jni::objects::JValue;
use jni::{jni_sig, jni_str};
use nikomiru_ui::session::IdleInhibitor;

/// `WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON`.
const FLAG_KEEP_SCREEN_ON: i32 = 0x80;

/// Asks Java's main thread to set or clear the flag.
#[derive(Debug, Default)]
pub struct PortalInhibitor;

impl PortalInhibitor {
    /// `reason` is for desktops that show one; Android does not.
    pub fn start(_reason: String) -> Self {
        Self
    }
}

impl IdleInhibitor for PortalInhibitor {
    fn set(&self, inhibit: bool) {
        let Some(app) = crate::platform::android::app() else {
            return;
        };
        app.run_on_java_main_thread(Box::new(move || {
            let result = crate::platform::android::with_activity(|env, activity| {
                let window = env
                    .call_method(
                        activity,
                        jni_str!("getWindow"),
                        jni_sig!("()Landroid/view/Window;"),
                        &[],
                    )?
                    .l()?;
                let method = if inhibit {
                    jni_str!("addFlags")
                } else {
                    jni_str!("clearFlags")
                };
                env.call_method(
                    &window,
                    method,
                    jni_sig!("(I)V"),
                    &[JValue::Int(FLAG_KEEP_SCREEN_ON)],
                )?;
                Ok(())
            });
            if let Err(e) = result {
                tracing::warn!("keeping the screen on: {e}");
            }
        }));
    }
}
