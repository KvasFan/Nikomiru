// SPDX-License-Identifier: GPL-3.0-only

//! The video, drawn with the libmpv OpenGL render API into the window's
//! framebuffer, under the interface (SPECIFICATION §5.2, ADR 0006).
//!
//! The UI thread is mpv's render thread: it calls only `mpv_render_*`
//! functions. Every other libmpv call runs on the command thread
//! ([`crate::PlayerHandle`]) or on the events thread, so the render thread
//! never waits for the core (`<mpv/render.h>`, "Threading").

use std::cell::Cell;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use libmpv2_sys as sys;

use crate::{Player, PlayerError};

/// The display of the window, needed for zero-copy hardware decoding (VA-API
/// through EGL, SPECIFICATION §5.2.2 #2).
#[derive(Debug, Clone, Copy)]
pub enum NativeDisplay {
    /// A Wayland `wl_display *`.
    Wayland(*mut c_void),
    /// An Xlib `Display *`.
    X11(*mut c_void),
    /// Not known: decoding falls back to copies.
    Unknown,
}

/// Resolves OpenGL function names (`eglGetProcAddress` and the like).
pub type GetProcAddress = Box<dyn Fn(&CStr) -> *const c_void>;

type UpdateCallback = Box<dyn Fn() + Send + Sync>;

/// The render context of the player's core: one per core, kept for every file
/// (PLY-009). It stays on the thread that created it.
pub struct VideoRenderer {
    ctx: *mut sys::mpv_render_context,
    // Handed to mpv as raw pointers; freed after the render context.
    get_proc_address: *mut GetProcAddress,
    update: *mut UpdateCallback,
}

impl std::fmt::Debug for VideoRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoRenderer").finish_non_exhaustive()
    }
}

fn param(kind: sys::mpv_render_param_type, data: *mut c_void) -> sys::mpv_render_param {
    sys::mpv_render_param { type_: kind, data }
}

fn end() -> sys::mpv_render_param {
    param(
        sys::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
        ptr::null_mut(),
    )
}

// mpv up to 0.41 makes a GL fence at the end of every frame drawn and deletes it
// only on a buffer swap of its own, which the render API never does: one fence
// left behind per frame, for good (T-REL-003; fixed upstream after 0.41 by
// f74adc4243, "require swap_buffers param for FenceSync"). mpv gets its
// `glFenceSync` and `glDeleteSync` through `fence_sync` and `delete_sync`, which
// note the frame's fence, and `render` deletes it once the call returns. The
// only other fences mpv makes are its upload buffers' (`ra_gl.c`), each made
// right after deleting the buffer's previous one: those are left alone, so with
// an mpv that makes no frame fence nothing is deleted.
type FenceSync = extern "C" fn(u32, u32) -> *mut c_void;
type DeleteSync = extern "C" fn(*mut c_void);

/// The driver's `glFenceSync` and `glDeleteSync`, as mpv resolved them.
static REAL_FENCE_SYNC: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static REAL_DELETE_SYNC: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

thread_local! {
    /// The frame's fence mpv made on this thread, in the current render call.
    static FRAME_FENCE: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    /// mpv's last sync call was a deletion: the next fence is a buffer's.
    static AFTER_DELETE: Cell<bool> = const { Cell::new(false) };
}

extern "C" fn fence_sync(condition: u32, flags: u32) -> *mut c_void {
    let real = REAL_FENCE_SYNC.load(Ordering::Acquire);
    // SAFETY: `resolve` hands out `fence_sync` only after storing the driver's
    // non-null `glFenceSync` here; mpv calls it with the GL context current.
    let fence = unsafe { std::mem::transmute::<*mut c_void, FenceSync>(real)(condition, flags) };
    if !AFTER_DELETE.replace(false) {
        FRAME_FENCE.set(fence);
    }
    fence
}

extern "C" fn delete_sync(sync: *mut c_void) {
    AFTER_DELETE.set(true);
    let real = REAL_DELETE_SYNC.load(Ordering::Acquire);
    // SAFETY: `resolve` hands out `delete_sync` only after storing the driver's
    // non-null `glDeleteSync` here; mpv calls it with the GL context current.
    unsafe { std::mem::transmute::<*mut c_void, DeleteSync>(real)(sync) }
}

extern "C" fn resolve(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    if ctx.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `ctx` is the `GetProcAddress` boxed by `VideoRenderer::new`, alive
    // until the render context is freed; `name` is a NUL-terminated string that
    // mpv keeps for the duration of the call.
    let (resolve, name) = unsafe { (&*ctx.cast::<GetProcAddress>(), CStr::from_ptr(name)) };
    let found = resolve(name).cast_mut();
    if found.is_null() {
        return found;
    }
    match name.to_bytes() {
        b"glFenceSync" => {
            REAL_FENCE_SYNC.store(found, Ordering::Release);
            let shim: FenceSync = fence_sync;
            shim as *mut c_void
        }
        b"glDeleteSync" => {
            REAL_DELETE_SYNC.store(found, Ordering::Release);
            let shim: DeleteSync = delete_sync;
            shim as *mut c_void
        }
        _ => found,
    }
}

extern "C" fn updated(ctx: *mut c_void) {
    // SAFETY: `ctx` is the `UpdateCallback` boxed by `VideoRenderer::new`; it is
    // freed only after `mpv_render_context_free`, after which mpv never calls it.
    let update = unsafe { &*ctx.cast::<UpdateCallback>() };
    update();
}

impl VideoRenderer {
    /// Creates the render context of `player`'s core. Files may be loaded only
    /// once it exists: with `vo=libmpv`, a file loaded earlier has no video
    /// output (ADR 0003). `on_update` runs on mpv's threads when a new frame is
    /// due or the picture must be redrawn: it must only schedule a redraw, never
    /// call mpv (SPECIFICATION §5.2.2 #4).
    ///
    /// # Safety
    ///
    /// - The OpenGL context the video is drawn with is current on this thread,
    ///   now and whenever this renderer is used or dropped.
    /// - `display` is the window's display and outlives the renderer.
    /// - The renderer is dropped before `player` shuts down (shutdown order of
    ///   SPECIFICATION §5.2.2 #7).
    pub unsafe fn new(
        player: &Player,
        display: NativeDisplay,
        get_proc_address: GetProcAddress,
        on_update: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, PlayerError> {
        let get_proc_address = Box::into_raw(Box::new(get_proc_address));
        let mut init = sys::mpv_opengl_init_params {
            get_proc_address: Some(resolve),
            get_proc_address_ctx: get_proc_address.cast(),
        };
        let mut advanced: c_int = 0;
        let mut params = vec![
            param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                sys::MPV_RENDER_API_TYPE_OPENGL.as_ptr().cast_mut().cast(),
            ),
            param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                (&raw mut init).cast(),
            ),
            param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_ADVANCED_CONTROL,
                (&raw mut advanced).cast(),
            ),
        ];
        match display {
            NativeDisplay::Wayland(d) => params.push(param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_WL_DISPLAY,
                d,
            )),
            NativeDisplay::X11(d) => params.push(param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_X11_DISPLAY,
                d,
            )),
            NativeDisplay::Unknown => {}
        }
        params.push(end());
        let mut ctx = ptr::null_mut();
        // SAFETY: `raw_handle` is the player's live core; the parameters point to
        // locals that outlive the call (mpv copies what it keeps) and end with
        // INVALID; the caller guarantees a current GL context and a valid display.
        let rc = unsafe {
            sys::mpv_render_context_create(
                &raw mut ctx,
                player.raw_handle().cast(),
                params.as_mut_ptr(),
            )
        };
        if rc < 0 {
            // SAFETY: mpv kept no reference to it: creation failed.
            drop(unsafe { Box::from_raw(get_proc_address) });
            return Err(PlayerError::Mpv(libmpv2::Error::Raw(rc)));
        }
        let on_update: UpdateCallback = Box::new(on_update);
        let update = Box::into_raw(Box::new(on_update));
        // SAFETY: `ctx` was just created; `update` stays valid until the context is
        // freed (Drop frees the context first).
        unsafe { sys::mpv_render_context_set_update_callback(ctx, Some(updated), update.cast()) };
        Ok(Self {
            ctx,
            get_proc_address,
            update,
        })
    }

    /// Draws the current frame into the framebuffer `fbo` (0: the window's own)
    /// of `width`×`height` physical pixels (SPECIFICATION §5.2.2 #5).
    /// `flip_y` is needed for the window's framebuffer. mpv draws the whole
    /// framebuffer, black bars included, and may block until the frame's
    /// display time. The OpenGL state must be the one mpv expects
    /// (`<mpv/render_gl.h>`: blending and scissor off, no program bound).
    pub fn render(&self, fbo: u32, width: i32, height: i32, flip_y: bool) {
        let mut target = sys::mpv_opengl_fbo {
            fbo: c_int::try_from(fbo).unwrap_or(0),
            w: width,
            h: height,
            internal_format: 0,
        };
        let mut flip = c_int::from(flip_y);
        let mut params = [
            param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                (&raw mut target).cast(),
            ),
            param(
                sys::mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                (&raw mut flip).cast(),
            ),
            end(),
        ];
        FRAME_FENCE.set(ptr::null_mut());
        // SAFETY: `ctx` is live and used on its own thread with its GL context
        // current (contract of `new`); the parameters outlive the call.
        let rc = unsafe { sys::mpv_render_context_render(self.ctx, params.as_mut_ptr()) };
        if rc < 0 {
            tracing::debug!(target: "mpv", "mpv_render_context_render: {rc}");
        }
        let fence = FRAME_FENCE.replace(ptr::null_mut());
        let delete = REAL_DELETE_SYNC.load(Ordering::Acquire);
        if !fence.is_null() && !delete.is_null() {
            // SAFETY: `delete` is the driver's `glDeleteSync`, the GL context is
            // current (contract of `new`), and `fence` is the frame's fence,
            // which mpv never waits on nor deletes (see `FenceSync`).
            unsafe { std::mem::transmute::<*mut c_void, DeleteSync>(delete)(fence) };
        }
    }

    /// Tells mpv that the frame drawn last is on screen: call it right after
    /// every buffer swap, for better timing.
    pub fn report_swap(&self) {
        // SAFETY: `ctx` is live and used on its own thread (contract of `new`).
        unsafe { sys::mpv_render_context_report_swap(self.ctx) }
    }
}

impl Drop for VideoRenderer {
    fn drop(&mut self) {
        // SAFETY: the context is live and its GL context current (contract of
        // `new`); once it is freed mpv calls neither callback, so their contexts
        // can be freed after it.
        unsafe {
            sys::mpv_render_context_free(self.ctx);
            drop(Box::from_raw(self.update));
            drop(Box::from_raw(self.get_proc_address));
        }
    }
}
