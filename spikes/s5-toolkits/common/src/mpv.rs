// SPDX-License-Identifier: GPL-3.0-only

//! libmpv through its C API: the client (options, commands, properties,
//! events) and the OpenGL render API, with the same options and render
//! parameters as S1q's `MpvItem`. The toolkit provides the GL context: the
//! render calls must run with it current.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[repr(C)]
struct MpvHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvRenderContext {
    _private: [u8; 0],
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvRenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvOpenGlInitParams {
    get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct MpvOpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;

const EVENT_NONE: c_int = 0;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const PARAM_INVALID: c_int = 0;
const PARAM_API_TYPE: c_int = 1;
const PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const PARAM_OPENGL_FBO: c_int = 3;
const PARAM_FLIP_Y: c_int = 4;
const PARAM_WL_DISPLAY: c_int = 9;
const PARAM_ADVANCED_CONTROL: c_int = 10;

const UPDATE_FRAME: u64 = 1;

#[link(name = "mpv")]
unsafe extern "C" {
    fn mpv_create() -> *mut MpvHandle;
    fn mpv_initialize(ctx: *mut MpvHandle) -> c_int;
    fn mpv_terminate_destroy(ctx: *mut MpvHandle);
    fn mpv_set_option_string(
        ctx: *mut MpvHandle,
        name: *const c_char,
        data: *const c_char,
    ) -> c_int;
    fn mpv_command(ctx: *mut MpvHandle, args: *mut *const c_char) -> c_int;
    fn mpv_get_property(
        ctx: *mut MpvHandle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    fn mpv_free(data: *mut c_void);
    fn mpv_observe_property(
        ctx: *mut MpvHandle,
        userdata: u64,
        name: *const c_char,
        format: c_int,
    ) -> c_int;
    fn mpv_wait_event(ctx: *mut MpvHandle, timeout: f64) -> *mut MpvEvent;
    fn mpv_set_wakeup_callback(
        ctx: *mut MpvHandle,
        cb: Option<unsafe extern "C" fn(*mut c_void)>,
        d: *mut c_void,
    );
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_render_context_create(
        res: *mut *mut MpvRenderContext,
        mpv: *mut MpvHandle,
        params: *mut MpvRenderParam,
    ) -> c_int;
    fn mpv_render_context_set_update_callback(
        ctx: *mut MpvRenderContext,
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        callback_ctx: *mut c_void,
    );
    fn mpv_render_context_update(ctx: *mut MpvRenderContext) -> u64;
    fn mpv_render_context_render(ctx: *mut MpvRenderContext, params: *mut MpvRenderParam) -> c_int;
    fn mpv_render_context_report_swap(ctx: *mut MpvRenderContext);
    fn mpv_render_context_free(ctx: *mut MpvRenderContext);
}

#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
}

#[link(name = "wayland-client")]
unsafe extern "C" {
    fn wl_display_connect(name: *const c_char) -> *mut c_void;
}

unsafe extern "C" {
    fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
}

/// `LC_NUMERIC` in glibc.
const LC_NUMERIC: c_int = 1;

unsafe extern "C" fn egl_proc_address(_: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { eglGetProcAddress(name) }
}

fn error(code: c_int) -> String {
    unsafe { CStr::from_ptr(mpv_error_string(code)) }
        .to_string_lossy()
        .into_owned()
}

/// What changed in mpv since the last poll.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    TimePos(f64),
    Pause(bool),
    FileLoaded,
    EndFile,
    Shutdown,
}

type Callback = Box<dyn Fn() + Send + Sync>;

/// An mpv core (thread-safe, as libmpv's handle is).
pub struct Mpv {
    handle: *mut MpvHandle,
    wakeup: *mut Callback,
}

unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

unsafe extern "C" fn call_back(data: *mut c_void) {
    // Called from mpv's threads: never call mpv here.
    let f = unsafe { &*(data as *const Callback) };
    f();
}

impl Mpv {
    /// A core with S1q's options (PLY-005), plus `S5_MPV_OPTIONS`
    /// (`key=value,key=value`). `wakeup` runs on mpv's threads when events
    /// are waiting.
    pub fn new(wakeup: impl Fn() + Send + Sync + 'static) -> Arc<Mpv> {
        unsafe { setlocale(LC_NUMERIC, c"C".as_ptr()) };
        let handle = unsafe { mpv_create() };
        assert!(!handle.is_null(), "mpv_create failed");
        let set = |k: &str, v: &str| {
            let (k, v) = (CString::new(k).unwrap(), CString::new(v).unwrap());
            unsafe { mpv_set_option_string(handle, k.as_ptr(), v.as_ptr()) };
        };
        for (k, v) in [
            ("vo", "libmpv"),
            ("config", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
            ("terminal", "no"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("ytdl", "no"),
            ("sub-auto", "no"),
            ("hwdec", "auto-safe"),
        ] {
            set(k, v);
        }
        if !std::path::Path::new("/proc/driver/nvidia/version").exists() {
            set("gpu-hwdec-interop", "vaapi");
        }
        if let Ok(extra) = std::env::var("S5_MPV_OPTIONS") {
            for kv in extra.split(',') {
                if let Some((k, v)) = kv.split_once('=') {
                    set(k, v);
                }
            }
        }
        let code = unsafe { mpv_initialize(handle) };
        assert!(code >= 0, "mpv_initialize: {}", error(code));
        for (name, format) in [("time-pos", FORMAT_DOUBLE), ("pause", FORMAT_FLAG)] {
            let name = CString::new(name).unwrap();
            unsafe { mpv_observe_property(handle, 0, name.as_ptr(), format) };
        }
        let wakeup: *mut Callback = Box::into_raw(Box::new(Box::new(wakeup)));
        unsafe { mpv_set_wakeup_callback(handle, Some(call_back), wakeup.cast()) };
        Arc::new(Mpv { handle, wakeup })
    }

    pub fn command(&self, args: &[&str]) {
        let owned: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap()).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|a| a.as_ptr()).collect();
        ptrs.push(ptr::null());
        let code = unsafe { mpv_command(self.handle, ptrs.as_mut_ptr()) };
        if code < 0 {
            eprintln!("[s5] mpv command {args:?}: {}", error(code));
        }
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        let name = CString::new(name).unwrap();
        let mut v: i64 = 0;
        let code = unsafe {
            mpv_get_property(
                self.handle,
                name.as_ptr(),
                FORMAT_INT64,
                (&raw mut v).cast(),
            )
        };
        (code >= 0).then_some(v)
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        let name = CString::new(name).unwrap();
        let mut v: f64 = 0.0;
        let code = unsafe {
            mpv_get_property(
                self.handle,
                name.as_ptr(),
                FORMAT_DOUBLE,
                (&raw mut v).cast(),
            )
        };
        (code >= 0).then_some(v)
    }

    pub fn get_string(&self, name: &str) -> Option<String> {
        let name = CString::new(name).unwrap();
        let mut v: *mut c_char = ptr::null_mut();
        let code = unsafe {
            mpv_get_property(
                self.handle,
                name.as_ptr(),
                FORMAT_STRING,
                (&raw mut v).cast(),
            )
        };
        if code < 0 || v.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(v) }.to_string_lossy().into_owned();
        unsafe { mpv_free(v.cast()) };
        Some(s)
    }

    /// The events waiting, without blocking.
    pub fn poll(&self) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            let e = unsafe { &*mpv_wait_event(self.handle, 0.0) };
            match e.event_id {
                EVENT_NONE => break,
                EVENT_SHUTDOWN => out.push(Event::Shutdown),
                EVENT_FILE_LOADED => out.push(Event::FileLoaded),
                EVENT_END_FILE => out.push(Event::EndFile),
                EVENT_PROPERTY_CHANGE => {
                    let p = unsafe { &*(e.data as *const MpvEventProperty) };
                    let name = unsafe { CStr::from_ptr(p.name) }.to_bytes();
                    match (name, p.format) {
                        (b"time-pos", FORMAT_DOUBLE) => {
                            out.push(Event::TimePos(unsafe { *(p.data as *const f64) }))
                        }
                        (b"pause", FORMAT_FLAG) => {
                            out.push(Event::Pause(unsafe { *(p.data as *const c_int) } != 0))
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        out
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            mpv_set_wakeup_callback(self.handle, None, ptr::null_mut());
            mpv_terminate_destroy(self.handle);
            drop(Box::from_raw(self.wakeup));
        }
    }
}

/// Counters of the render calls, read by the measurements.
#[derive(Default)]
pub struct RenderStats {
    pub renders: AtomicU64,
    pub new_frames: AtomicU64,
    pub times_us: std::sync::Mutex<Vec<u32>>,
    pub size: std::sync::Mutex<(i32, i32)>,
}

impl RenderStats {
    /// p50, p95 and max of the render times, in microseconds.
    pub fn percentiles(&self) -> (u32, u32, u32) {
        let mut t = self.times_us.lock().unwrap().clone();
        if t.is_empty() {
            return (0, 0, 0);
        }
        t.sort_unstable();
        let at = |q: f64| t[((t.len() as f64 * q).ceil() as usize).clamp(1, t.len()) - 1];
        (at(0.5), at(0.95), *t.last().unwrap())
    }
}

/// mpv's OpenGL renderer.
pub struct Renderer {
    ctx: *mut MpvRenderContext,
    update: *mut Callback,
    pub stats: Arc<RenderStats>,
    _mpv: Arc<Mpv>,
}

unsafe impl Send for Renderer {}
unsafe impl Sync for Renderer {}

impl Renderer {
    /// # Safety
    ///
    /// An EGL context must be current on this thread; it must stay alive
    /// (and be current) whenever this renderer renders or is dropped.
    pub unsafe fn new(
        mpv: Arc<Mpv>,
        stats: Arc<RenderStats>,
        on_update: impl Fn() + Send + Sync + 'static,
    ) -> Result<Renderer, String> {
        let mut init = MpvOpenGlInitParams {
            get_proc_address: egl_proc_address,
            get_proc_address_ctx: ptr::null_mut(),
        };
        let mut advanced: c_int = 0;
        // our own connection to the compositor, for VA-API without copies
        // (S1q passes Qt's; VA only needs one to find the device)
        let wl = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            unsafe { wl_display_connect(ptr::null()) }
        } else {
            ptr::null_mut()
        };
        let mut params = vec![
            MpvRenderParam {
                kind: PARAM_API_TYPE,
                data: c"opengl".as_ptr() as *mut c_void,
            },
            MpvRenderParam {
                kind: PARAM_OPENGL_INIT_PARAMS,
                data: (&raw mut init).cast(),
            },
            MpvRenderParam {
                kind: PARAM_ADVANCED_CONTROL,
                data: (&raw mut advanced).cast(),
            },
        ];
        if !wl.is_null() {
            params.push(MpvRenderParam {
                kind: PARAM_WL_DISPLAY,
                data: wl,
            });
        }
        params.push(MpvRenderParam {
            kind: PARAM_INVALID,
            data: ptr::null_mut(),
        });
        let mut ctx = ptr::null_mut();
        let code = unsafe { mpv_render_context_create(&mut ctx, mpv.handle, params.as_mut_ptr()) };
        if code < 0 {
            return Err(error(code));
        }
        let update: *mut Callback = Box::into_raw(Box::new(Box::new(on_update)));
        unsafe { mpv_render_context_set_update_callback(ctx, Some(call_back), update.cast()) };
        Ok(Renderer {
            ctx,
            update,
            stats,
            _mpv: mpv,
        })
    }

    /// Renders the current video frame into `fbo` (`w`×`h`); true if it was
    /// a new frame.
    ///
    /// # Safety
    ///
    /// The context given to [`Renderer::new`] must be current.
    pub unsafe fn render(&self, fbo: i32, w: i32, h: i32, flip_y: bool) -> bool {
        let t = Instant::now();
        let new_frame = unsafe { mpv_render_context_update(self.ctx) } & UPDATE_FRAME != 0;
        let mut target = MpvOpenGlFbo {
            fbo,
            w,
            h,
            internal_format: 0,
        };
        let mut flip: c_int = flip_y as c_int;
        let mut params = [
            MpvRenderParam {
                kind: PARAM_OPENGL_FBO,
                data: (&raw mut target).cast(),
            },
            MpvRenderParam {
                kind: PARAM_FLIP_Y,
                data: (&raw mut flip).cast(),
            },
            MpvRenderParam {
                kind: PARAM_INVALID,
                data: ptr::null_mut(),
            },
        ];
        unsafe { mpv_render_context_render(self.ctx, params.as_mut_ptr()) };
        let s = &self.stats;
        s.renders.fetch_add(1, Ordering::Relaxed);
        if new_frame {
            s.new_frames.fetch_add(1, Ordering::Relaxed);
        }
        s.times_us
            .lock()
            .unwrap()
            .push(t.elapsed().as_micros() as u32);
        *s.size.lock().unwrap() = (w, h);
        new_frame
    }

    /// After the frame with the video went to the screen.
    pub fn report_swap(&self) {
        unsafe { mpv_render_context_report_swap(self.ctx) };
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            mpv_render_context_set_update_callback(self.ctx, None, ptr::null_mut());
            mpv_render_context_free(self.ctx);
            drop(Box::from_raw(self.update));
        }
    }
}
