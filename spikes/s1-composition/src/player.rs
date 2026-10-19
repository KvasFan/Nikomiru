// SPDX-License-Identifier: GPL-3.0-only

//! mpv with the OpenGL render API inside the GtkGLArea (SPECIFICATION §5.2.2) and
//! a thread for events (§5.3). The spike code accepts synchronous property reads
//! only in the measurements; from M1 on, ARC-003 applies.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_channel::mpsc::UnboundedSender;
use gtk::prelude::*;
use libmpv2::events::{Event, PropertyData};
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::{Format, Mpv};

use crate::compositor::{self, GlApi, NativeDisplay};

/// Messages from the player to the Dioxus UI.
#[derive(Debug, Clone)]
pub enum UiMsg {
    Time(f64),
    Paused(bool),
    Info(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct Options {
    pub video: PathBuf,
    pub hwdec: String,
    /// Extra mpv options as `key=value`, applied last (as in PLY-006).
    pub extra: Vec<(String, String)>,
}

type GetIntegerv = unsafe extern "C" fn(u32, *mut i32);
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;

thread_local! {
    static PLAYER: RefCell<Option<Player>> = const { RefCell::new(None) };
    /// Frames drawn by the GLArea (for stress mode).
    pub static RENDERS: Cell<u64> = const { Cell::new(0) };
    /// Last render size in physical pixels (to check HiDPI).
    pub static RENDER_SIZE: Cell<(i32, i32, i32)> = const { Cell::new((0, 0, 0)) };
}

pub struct Player {
    /// Destroyed first, with the GL context current (see `Drop`).
    render: Option<RenderContext<'static>>,
    mpv: Option<Box<Mpv>>,
    events: Option<JoinHandle<()>>,
    get_integerv: Option<GetIntegerv>,
}

impl Player {
    fn new(area: &gtk::GLArea, opts: &Options, tx: UnboundedSender<UiMsg>) -> Result<Self, String> {
        let (api, display) = compositor::detect();
        compositor::force_c_numeric_locale();
        // PLY-005: fixed options before initialize.
        let mpv = Mpv::with_initializer(|init| {
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
                ("hwdec", opts.hwdec.as_str()),
            ] {
                if let Err(e) = init.set_property(k, v) {
                    eprintln!("[s1] mpv option {k}={v} rejected: {e}");
                }
            }
            for (k, v) in &opts.extra {
                if let Err(e) = init.set_property(k.as_str(), v.as_str()) {
                    eprintln!("[s1] mpv option {k}={v} rejected: {e}");
                }
            }
            Ok(())
        })
        .map_err(|e| format!("mpv: {e}"))?;
        let mpv = Box::new(mpv);

        let mut params = vec![
            RenderParam::ApiType(RenderParamApiType::OpenGl),
            RenderParam::InitParams(OpenGLInitParams {
                get_proc_address: compositor::get_proc_address,
                ctx: api,
            }),
        ];
        match display {
            Some(NativeDisplay::Wayland(p)) => params.push(RenderParam::WaylandDisplay(p)),
            Some(NativeDisplay::X11(p)) => params.push(RenderParam::X11Display(p)),
            None => {}
        }
        let render = mpv
            .create_render_context(params)
            .map_err(|e| format!("render context: {e}"))?;
        // SAFETY: `RenderContext<'a>` ties `'a` to `Mpv` only through a PhantomData;
        // the pointer it holds stays valid as long as the mpv core is alive. `Player`
        // keeps `mpv` in a Box (stable address) and its Drop destroys the render
        // context before `mpv`.
        let mut render: RenderContext<'static> = unsafe { std::mem::transmute(render) };

        let weak = glib::SendWeakRef::from(area.downgrade());
        render.set_update_callback(move || {
            // Never call the mpv API here (SPECIFICATION §5.2.2): only a redraw on the GTK thread.
            let weak = weak.clone();
            glib::MainContext::default().invoke(move || {
                if let Some(a) = weak.upgrade() {
                    if std::env::var_os("S1_DEBUG").is_some() {
                        eprintln!(
                            "[s1] update → queue_render (mapped {}, {}x{})",
                            a.is_mapped(),
                            a.allocated_width(),
                            a.allocated_height()
                        );
                    }
                    a.queue_render();
                }
            });
        });

        let p = compositor::get_proc_address(&api, "glGetIntegerv");
        let get_integerv = (!p.is_null()).then(|| {
            // SAFETY: `p` is the address of glGetIntegerv returned by the GL loader
            // for the current context; the signature matches the OpenGL one.
            unsafe { std::mem::transmute::<*mut c_void, GetIntegerv>(p) }
        });

        // `create_client(None)`: with a name, libmpv2 6.0 passes mpv a pointer to a
        // temporary CString that has already been freed.
        let client = mpv
            .create_client(None)
            .map_err(|e| format!("client: {e}"))?;
        for (name, fmt) in [
            ("time-pos", Format::Double),
            ("pause", Format::Flag),
            ("hwdec-current", Format::String),
            ("frame-drop-count", Format::Int64),
            ("vo-delayed-frame-count", Format::Int64),
            ("estimated-vf-fps", Format::Double),
        ] {
            client
                .observe_property(name, fmt, 0)
                .map_err(|e| format!("observe {name}: {e}"))?;
        }
        let events = std::thread::Builder::new()
            .name("mpv-events".into())
            .spawn(move || event_loop(client, tx))
            .map_err(|e| e.to_string())?;

        let _ = api;
        Ok(Self {
            render: Some(render),
            mpv: Some(mpv),
            events: Some(events),
            get_integerv,
        })
    }

    fn mpv(&self) -> Option<&Mpv> {
        self.mpv.as_deref()
    }

    fn render(&self, area: &gtk::GLArea) {
        let Some(render) = &self.render else { return };
        let mut fbo = 0i32;
        if let Some(f) = self.get_integerv {
            // SAFETY: GTK makes the GLArea's context current before emitting
            // "render"; `fbo` is a valid integer to write into.
            unsafe { f(GL_DRAW_FRAMEBUFFER_BINDING, &mut fbo) };
        }
        let scale = area.scale_factor();
        let (w, h) = (
            area.allocated_width() * scale,
            area.allocated_height() * scale,
        );
        let _ = render.update();
        if let Err(e) = render.render::<GlApi>(fbo, w, h, true) {
            eprintln!("[s1] render: {e}");
        }
        render.report_swap();
        RENDERS.set(RENDERS.get() + 1);
        RENDER_SIZE.set((w, h, scale));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Mandatory order: render context (with the GL context current) → quit the
        // core, which sends SHUTDOWN to the event client → join the thread → main handle.
        self.render.take();
        if let Some(mpv) = &self.mpv {
            let _ = mpv.command("quit", &[]);
        }
        if let Some(h) = self.events.take() {
            let _ = h.join();
        }
        self.mpv.take();
    }
}

fn event_loop(client: Mpv, tx: UnboundedSender<UiMsg>) {
    let mut pending: Option<f64> = None;
    let mut last_sent = Instant::now() - Duration::from_secs(1);
    let mut last_stats = Instant::now();
    let (mut hwdec, mut drops, mut delayed, mut fps) = (String::new(), 0i64, 0i64, 0.0f64);
    loop {
        match client.wait_event(0.01) {
            None => {}
            Some(Ok(Event::Shutdown)) => break,
            Some(Ok(Event::PropertyChange { name, change, .. })) => match (name, change) {
                ("time-pos", PropertyData::Double(t)) => pending = Some(t),
                ("pause", PropertyData::Flag(p)) => {
                    let _ = tx.unbounded_send(UiMsg::Paused(p));
                }
                ("hwdec-current", PropertyData::Str(s)) => hwdec = s.to_string(),
                ("frame-drop-count", PropertyData::Int64(n)) => drops = n,
                ("vo-delayed-frame-count", PropertyData::Int64(n)) => delayed = n,
                ("estimated-vf-fps", PropertyData::Double(f)) => fps = f,
                _ => {}
            },
            Some(Ok(Event::FileLoaded)) => {
                let _ = tx.unbounded_send(UiMsg::Info("file-loaded".into()));
            }
            Some(Ok(Event::EndFile(_))) => {
                let _ = tx.unbounded_send(UiMsg::Info("end-file".into()));
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                let _ = tx.unbounded_send(UiMsg::Error(e.to_string()));
            }
        }
        // ARC-004: at most 30 updates per second, always with the latest value.
        if let Some(t) = pending
            && last_sent.elapsed() >= Duration::from_millis(33)
        {
            let _ = tx.unbounded_send(UiMsg::Time(t));
            pending = None;
            last_sent = Instant::now();
        }
        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            let _ = tx.unbounded_send(UiMsg::Info(format!(
                "hwdec {} · {fps:.1} fps · dropped {drops} · delayed {delayed}",
                if hwdec.is_empty() { "no" } else { &hwdec }
            )));
        }
    }
}

/// Connects the GLArea: realize creates the player, unrealize destroys it.
pub fn attach(area: &gtk::GLArea, opts: Options, tx: UnboundedSender<UiMsg>) {
    area.connect_realize(move |area| {
        area.make_current();
        if let Some(e) = area.error() {
            let _ = tx.unbounded_send(UiMsg::Error(format!("GLArea: {e}")));
            return;
        }
        match Player::new(area, &opts, tx.clone()) {
            Ok(p) => {
                if let Some(mpv) = p.mpv() {
                    let path = opts.video.to_string_lossy().into_owned();
                    if let Err(e) = mpv.command("loadfile", &[&path, "replace"]) {
                        let _ = tx.unbounded_send(UiMsg::Error(format!("loadfile: {e}")));
                    }
                }
                PLAYER.set(Some(p));
            }
            Err(e) => {
                eprintln!("[s1] player: {e}");
                let _ = tx.unbounded_send(UiMsg::Error(e));
            }
        }
    });
    area.connect_unrealize(|area| {
        area.make_current();
        let player = PLAYER.take();
        drop(player);
    });
    area.connect_render(|area, _ctx| {
        if std::env::var_os("S1_DEBUG").is_some() {
            eprintln!(
                "[s1] render {}x{} err {:?}",
                area.allocated_width(),
                area.allocated_height(),
                area.error()
            );
        }
        PLAYER.with_borrow(|p| {
            if let Some(p) = p {
                p.render(area);
            }
        });
        glib::Propagation::Stop
    });
}

/// mpv command from the UI thread (the spike's buttons and keys).
pub fn command(args: &[&str]) {
    PLAYER.with_borrow(|p| {
        if let (Some(mpv), Some((name, rest))) =
            (p.as_ref().and_then(Player::mpv), args.split_first())
        {
            let _ = mpv.command(name, rest);
        }
    });
}

/// Synchronous read: only for the spike's measurements.
pub fn get_i64(name: &str) -> Option<i64> {
    PLAYER.with_borrow(|p| {
        p.as_ref()
            .and_then(Player::mpv)?
            .get_property::<i64>(name)
            .ok()
    })
}

pub fn get_f64(name: &str) -> Option<f64> {
    PLAYER.with_borrow(|p| {
        p.as_ref()
            .and_then(Player::mpv)?
            .get_property::<f64>(name)
            .ok()
    })
}

pub fn get_string(name: &str) -> Option<String> {
    PLAYER.with_borrow(|p| {
        p.as_ref()
            .and_then(Player::mpv)?
            .get_property::<String>(name)
            .ok()
    })
}

pub fn is_active() -> bool {
    PLAYER.with_borrow(Option::is_some)
}
