// SPDX-License-Identifier: GPL-3.0-only

//! Files and links (APP-003, POP-011) on Android (D-16): files are chosen
//! in the app's own chooser (the shell opens it, not this module); links
//! open in the browser through an `ACTION_VIEW` intent; reading the shared
//! storage needs "all files access", asked for once in the system settings.

use std::sync::Arc;

use jni::objects::{JObject, JValue};
use jni::{Env, jni_sig, jni_str};
use nikomiru_ui::{FileDialog, Inbound, Waker, is_web_address};

use crate::platform::android::with_activity;

/// Not used on Android: the shell shows the app's own chooser instead.
pub fn open_file(_dialog: FileDialog, waker: Arc<dyn Waker>) {
    waker.wake(Inbound::DialogFailed);
}

/// Opens a web address from a dictionary in the browser (POP-011). Anything
/// but `http(s)` is refused again here.
pub fn open_uri(url: &str) {
    if !is_web_address(url) {
        tracing::warn!("not a web address, not opened");
        return;
    }
    if let Err(e) = with_activity(|env, activity| {
        let uri = parse_uri(env, url)?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        start_activity(env, activity, &intent)
    }) {
        tracing::warn!("opening a link: {e}");
    }
}

/// Whether the app may read every file of the shared storage (Android 11
/// and later ask for it in the settings; before, the manifest's read
/// permission is enough).
pub fn can_read_all_files() -> bool {
    with_activity(|env, _| {
        if sdk(env)? < 30 {
            return Ok(true);
        }
        let granted = env
            .call_static_method(
                jni_str!("android/os/Environment"),
                jni_str!("isExternalStorageManager"),
                jni_sig!("()Z"),
                &[],
            )?
            .z()?;
        Ok(granted)
    })
    .unwrap_or(false)
}

/// Opens the system settings where the user lets the app read all files.
pub fn ask_to_read_all_files() {
    if let Err(e) = with_activity(|env, activity| {
        let package = env
            .call_method(
                activity,
                jni_str!("getPackageName"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let package = env.cast_local::<jni::objects::JString>(package)?;
        let package = package.mutf8_chars(env)?.to_string();
        let uri = parse_uri(env, &format!("package:{package}"))?;
        let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        start_activity(env, activity, &intent)
    }) {
        tracing::warn!("asking for all files access: {e}");
    }
}

fn parse_uri<'local>(env: &mut Env<'local>, text: &str) -> jni::errors::Result<JObject<'local>> {
    let text = env.new_string(text)?;
    env.call_static_method(
        jni_str!("android/net/Uri"),
        jni_str!("parse"),
        jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
        &[JValue::Object(&text)],
    )?
    .l()
}

fn start_activity(
    env: &mut Env<'_>,
    activity: &JObject<'_>,
    intent: &JObject<'_>,
) -> jni::errors::Result<()> {
    env.call_method(
        activity,
        jni_str!("startActivity"),
        jni_sig!("(Landroid/content/Intent;)V"),
        &[JValue::Object(intent)],
    )?;
    Ok(())
}

fn sdk(env: &mut Env<'_>) -> jni::errors::Result<i32> {
    env.get_static_field(
        jni_str!("android/os/Build$VERSION"),
        jni_str!("SDK_INT"),
        jni_sig!("I"),
    )?
    .i()
}
