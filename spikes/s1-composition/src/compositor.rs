// SPDX-License-Identifier: GPL-3.0-only

//! Composition from SPECIFICATION §5.2.1, strategy (a): at runtime wry's
//! WebKitWebView is removed from tao's `default_vbox` and added back as the
//! overlay of a `GtkOverlay` whose main child is mpv's `GtkGLArea`.
//!
//! ```text
//! GtkWindow (tao) └── GtkBox default_vbox └── GtkOverlay
//!                                            ├── GtkGLArea        (video, below)
//!                                            └── WebKitWebView    (UI, above, transparent)
//! ```

use std::ffi::{CString, c_char, c_void};

use dioxus::desktop::DesktopContext;
use dioxus::desktop::tao::platform::unix::WindowExtUnix;
use dioxus::desktop::wry::WebViewExtUnix;
use glib::translate::ToGlibPtr;
use gtk::prelude::*;

/// The API GDK used to create the GLArea's GL context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlApi {
    /// Wayland: EGL.
    Egl,
    /// X11 with GTK 3.24: GLX.
    Glx,
}

/// The native display, for mpv's hwdec interop (VA-API).
#[derive(Debug, Clone, Copy)]
pub enum NativeDisplay {
    Wayland(*const c_void),
    X11(*const c_void),
}

#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
}

#[link(name = "GL")]
unsafe extern "C" {
    fn glXGetProcAddressARB(name: *const u8) -> *mut c_void;
}

unsafe extern "C" {
    fn setlocale(category: i32, locale: *const c_char) -> *mut c_char;
}

/// glibc's `LC_NUMERIC`.
const LC_NUMERIC: i32 = 1;

/// libmpv refuses to start if `LC_NUMERIC` is not "C"; `gtk_init` sets it from
/// the environment, so it must be reset to "C" after GTK and before `mpv_create`.
pub fn force_c_numeric_locale() {
    // SAFETY: the string is a NUL-terminated literal; this is called on the main
    // thread, before mpv is created, and no other thread reads the locale.
    unsafe { setlocale(LC_NUMERIC, c"C".as_ptr()) };
}

#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_wayland_display_get_wl_display(display: *mut c_void) -> *mut c_void;
    fn gdk_x11_display_get_xdisplay(display: *mut c_void) -> *mut c_void;
}

/// `get_proc_address` for mpv's render context: direct FFI into Mesa, without a
/// loader crate (SPECIFICATION §5.2.2).
pub fn get_proc_address(api: &GlApi, name: &str) -> *mut c_void {
    let Ok(name) = CString::new(name) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `name` is a valid C string for the whole call; both functions
    // accept any name and return NULL if they do not know it.
    unsafe {
        match api {
            GlApi::Egl => eglGetProcAddress(name.as_ptr()),
            GlApi::Glx => glXGetProcAddressARB(name.as_ptr().cast()),
        }
    }
}

/// GL API and native display of the default GDK display.
pub fn detect() -> (GlApi, Option<NativeDisplay>) {
    let Some(display) = gdk::Display::default() else {
        return (GlApi::Egl, None);
    };
    let ty = display.type_().name();
    let raw: *mut gdk::ffi::GdkDisplay = display.to_glib_none().0;
    match ty {
        "GdkWaylandDisplay" => {
            // SAFETY: `raw` is a live GdkWaylandDisplay (the default display).
            let wl = unsafe { gdk_wayland_display_get_wl_display(raw.cast()) };
            (
                GlApi::Egl,
                (!wl.is_null()).then_some(NativeDisplay::Wayland(wl)),
            )
        }
        "GdkX11Display" => {
            // SAFETY: `raw` is a live GdkX11Display (the default display).
            let x = unsafe { gdk_x11_display_get_xdisplay(raw.cast()) };
            (GlApi::Glx, (!x.is_null()).then_some(NativeDisplay::X11(x)))
        }
        _ => (GlApi::Egl, None),
    }
}

/// Reparents the webview and creates the GLArea; `prepare` connects the GLArea's
/// signals before it is shown (and therefore realized). Returns the overlay so
/// that the GLArea can be replaced later (stress mode).
pub fn install(
    desktop: &DesktopContext,
    prepare: impl FnOnce(&gtk::GLArea),
) -> Result<(gtk::Overlay, gtk::GLArea), String> {
    let vbox = desktop
        .window
        .default_vbox()
        .ok_or("tao did not create the default_vbox")?;
    let webview = desktop.webview.webview();
    let children = vbox.children();
    if !children
        .iter()
        .any(|c| c == webview.upcast_ref::<gtk::Widget>())
    {
        return Err(format!(
            "the webview is not a direct child of the vbox (children: {:?})",
            children
                .iter()
                .map(|c| c.type_().name())
                .collect::<Vec<_>>()
        ));
    }
    vbox.remove(&webview);
    let overlay = gtk::Overlay::new();
    let area = new_gl_area();
    prepare(&area);
    overlay.add(&area);
    overlay.add_overlay(&webview);
    vbox.pack_start(&overlay, true, true, 0);
    overlay.show_all();
    // S1_NO_WEBVIEW: hide the webview to measure the GLArea alone.
    if std::env::var_os("S1_NO_WEBVIEW").is_some() {
        webview.set_visible(false);
    }
    vbox.queue_resize();
    webview.grab_focus();
    Ok((overlay, area))
}

/// GLArea for mpv: no alpha, rendering only when mpv requests it.
pub fn new_gl_area() -> gtk::GLArea {
    let area = gtk::GLArea::new();
    area.set_auto_render(false);
    area.set_has_alpha(false);
    area.set_has_depth_buffer(false);
    area.set_hexpand(true);
    area.set_vexpand(true);
    area
}

/// Replaces the overlay's GLArea (the old one is destroyed, which unrealizes it).
pub fn replace_gl_area(
    overlay: &gtk::Overlay,
    old: &gtk::GLArea,
    prepare: impl FnOnce(&gtk::GLArea),
) -> gtk::GLArea {
    overlay.remove(old);
    // SAFETY: nothing uses `old` after its removal: it is destroyed right away.
    unsafe { old.destroy() };
    let area = new_gl_area();
    prepare(&area);
    overlay.add(&area);
    area.show();
    area
}

/// Description of the environment, for the reports.
pub fn describe(desktop: &DesktopContext) -> serde_json::Value {
    let display = gdk::Display::default();
    let ty = display.as_ref().map(|d| d.type_().name().to_string());
    let scale = desktop.window.gtk_window().scale_factor();
    serde_json::json!({
        "gdk_display": ty,
        "gdk_backend_env": std::env::var("GDK_BACKEND").ok(),
        "webkit_disable_dmabuf_env": std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").ok(),
        "xdg_session_type": std::env::var("XDG_SESSION_TYPE").ok(),
        "scale_factor": scale,
    })
}

/// Prints the widget tree with allocations (spike diagnostics).
pub fn dump_tree(w: &gtk::Widget, depth: usize) {
    let a = w.allocation();
    eprintln!(
        "[s1] {}{} {}x{}+{}+{} visible={} mapped={} hexpand={} vexpand={}",
        "  ".repeat(depth),
        w.type_().name(),
        a.width(),
        a.height(),
        a.x(),
        a.y(),
        w.is_visible(),
        w.is_mapped(),
        w.hexpands(),
        w.vexpands()
    );
    if let Some(c) = w.dynamic_cast_ref::<gtk::Container>() {
        for child in c.children() {
            dump_tree(&child, depth + 1);
        }
    }
}
