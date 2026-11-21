// SPDX-License-Identifier: GPL-3.0-only

//! What Android adds to the Linux calls (D-16, ADR 0009): the activity the
//! app runs in, the folders Android gives it, the Java VM handed to FFmpeg,
//! FFmpeg's programs, AnkiDroid's collection, and no drag and drop.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub mod ankidroid;

pub use winit::platform::android::activity::AndroidApp;

/// The activity, set once by the library's `android_main`.
static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Keeps the activity for the event loop and the platform layers, sets the
/// folders the app writes to, and hands the Java VM to FFmpeg. Call it once,
/// first thing in `android_main`.
pub fn start(app: AndroidApp) {
    let files = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/nikomiru"));
    set_folders(&files);
    give_java_vm_to_ffmpeg(app.vm_as_ptr());
    if APP.set(app).is_err() {
        tracing::warn!("the Android activity was set twice");
    }
    find_ffmpeg(&files);
    nikomiru_anki::bridge::install(Box::new(nikomiru_anki::bridge::Dispatcher::new(
        ankidroid::AnkiDroid,
    )));
}

/// FFmpeg's programs come in the APK as `libffmpeg.so` and `libffprobe.so`
/// beside the app's libraries, the one place Android lets an app run its
/// own programs from (`packaging/android/ffmpeg.sh`). Links named `ffmpeg`
/// and `ffprobe` in `files/bin` point at them, and `NIKOMIRU_FFMPEG` at that
/// folder (EXT-001).
fn find_ffmpeg(files: &Path) {
    let libraries = match with_activity(|env, activity| {
        let info = env
            .call_method(
                activity,
                jni::jni_str!("getApplicationInfo"),
                jni::jni_sig!("()Landroid/content/pm/ApplicationInfo;"),
                &[],
            )?
            .l()?;
        let dir = env
            .get_field(
                &info,
                jni::jni_str!("nativeLibraryDir"),
                jni::jni_sig!("Ljava/lang/String;"),
            )?
            .l()?;
        let dir = env.cast_local::<jni::objects::JString>(dir)?;
        dir.try_to_string(env)
    }) {
        Ok(dir) => PathBuf::from(dir),
        Err(e) => {
            tracing::warn!("the app's library folder: {e}");
            return;
        }
    };
    if !libraries.join("libffmpeg.so").is_file() {
        tracing::info!("this APK has no FFmpeg");
        return;
    }
    let bin = files.join("bin");
    let linked = std::fs::create_dir_all(&bin).and_then(|()| {
        for (name, library) in [("ffmpeg", "libffmpeg.so"), ("ffprobe", "libffprobe.so")] {
            let link = bin.join(name);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(libraries.join(library), &link)?;
        }
        Ok(())
    });
    match linked {
        // SAFETY: still at start, before any other thread of the app exists.
        Ok(()) => unsafe { std::env::set_var("NIKOMIRU_FFMPEG", &bin) },
        Err(e) => tracing::warn!("{}: {e}", bin.display()),
    }
}

/// The activity, once [`start`] has run.
pub fn app() -> Option<AndroidApp> {
    APP.get().cloned()
}

/// The folders of Nikomiru's settings, data, cache and state, all in the
/// app's own storage, named as on Linux so `Paths` finds them.
fn set_folders(files: &Path) {
    let pairs = [
        ("HOME", files.to_path_buf()),
        ("XDG_CONFIG_HOME", files.join("config")),
        ("XDG_DATA_HOME", files.join("data")),
        ("XDG_CACHE_HOME", files.join("cache")),
        ("XDG_STATE_HOME", files.join("state")),
    ];
    for (name, path) in pairs {
        if let Err(e) = std::fs::create_dir_all(&path) {
            eprintln!("nikomiru: {}: {e}", path.display());
        }
        // SAFETY: called once at start, before any other thread of the app
        // exists, so nothing reads the environment meanwhile.
        unsafe { std::env::set_var(name, &path) };
    }
    // a file `debug-log` in the app's folder turns on the debug log, as
    // `--debug` does on the desktop (Android has no command line)
    if files.join("debug-log").exists() {
        // SAFETY: as above.
        unsafe { std::env::set_var("NIKOMIRU_LOG", "debug") };
    }
}

/// FFmpeg's Android decoders and mpv's AudioTrack output reach Java through
/// the VM FFmpeg was given (`av_jni_set_java_vm`). libavcodec is loaded with
/// libmpv; without it, nothing to give.
fn give_java_vm_to_ffmpeg(vm: *mut std::ffi::c_void) {
    // SAFETY: only the type of FFmpeg's function, called below.
    type SetJavaVm = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
    // SAFETY: a lookup by name in the libraries already loaded.
    let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"av_jni_set_java_vm".as_ptr()) };
    if symbol.is_null() || vm.is_null() {
        return;
    }
    // SAFETY: the symbol is FFmpeg's `int av_jni_set_java_vm(void *vm, void
    // *log_ctx)`; the VM lives as long as the process.
    let set: SetJavaVm = unsafe { std::mem::transmute(symbol) };
    // SAFETY: as above; no log context.
    let rc = unsafe { set(vm, std::ptr::null_mut()) };
    if rc != 0 {
        eprintln!("nikomiru: av_jni_set_java_vm: {rc}");
    }
}

/// Runs `act` with Java attached to this thread and the activity at hand.
pub fn with_activity<T>(
    act: impl FnOnce(&mut jni::Env<'_>, &jni::objects::JObject<'_>) -> jni::errors::Result<T>,
) -> Result<T, String> {
    let app = app().ok_or("no activity")?;
    // SAFETY: the VM pointer is the one Android gave the activity; it lives
    // as long as the process.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let raw = app.activity_as_ptr().cast();
        // SAFETY: a global reference to the activity, kept by
        // android-activity for the life of the app.
        let activity = unsafe { jni::objects::JObject::from_raw(env, raw) };
        // the reference stays android-activity's: a `JObject` deletes nothing
        act(env, &activity)
    })
    .map_err(|e: jni::errors::Error| e.to_string())
}

/// The parts of a window of `size` pixels the system bars cover, in points
/// of `pixels_per_point`: the window less the activity's content rectangle.
pub fn safe_area(
    size: winit::dpi::PhysicalSize<u32>,
    pixels_per_point: f32,
) -> Option<egui::SafeAreaInsets> {
    let rect = app()?.content_rect();
    if rect.right <= rect.left || rect.bottom <= rect.top || pixels_per_point <= 0.0 {
        return None;
    }
    let (width, height) = (
        i32::try_from(size.width).ok()?,
        i32::try_from(size.height).ok()?,
    );
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        tracing::info!(
            "window {width}x{height}, content {},{} – {},{}",
            rect.left,
            rect.top,
            rect.right,
            rect.bottom
        );
    });
    let points = |pixels: i32| pixels.clamp(0, 4096) as f32 / pixels_per_point;
    Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
        left: points(rect.left),
        top: points(rect.top),
        right: points(width - rect.right),
        bottom: points(height - rect.bottom),
    }))
}

/// Drag and drop: none on Android (files come from the in-app chooser).
pub mod dnd {
    use std::path::PathBuf;
    use std::sync::Arc;

    use winit::window::Window;

    /// What a drop calls with its files.
    pub type OnDrop = Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>;

    /// Nothing to listen to.
    #[derive(Debug)]
    pub struct Drops;

    impl Drops {
        /// Always `None`.
        pub fn new(_window: &Window, _on_drop: OnDrop) -> Option<Self> {
            None
        }

        /// Nothing to dispatch.
        pub fn dispatch(&mut self) {}
    }

    /// Nothing to release; the shell drops it before the event loop, as on
    /// Linux.
    impl Drop for Drops {
        fn drop(&mut self) {}
    }
}
