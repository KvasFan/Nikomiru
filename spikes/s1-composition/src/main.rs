// SPDX-License-Identifier: GPL-3.0-only

//! Spike S1 (ROADMAP M0): mpv video and the Dioxus UI in the same window, using
//! the solution from SPECIFICATION §5.2.1 (GtkOverlay: GLArea below, transparent
//! webview above). Throwaway code: in M1 it is rewritten cleanly in
//! `anikoto-player` and `anikoto-app::linux`.
//!
//! Usage:
//!   spike-s1-composition <video> [--hwdec auto-safe] [--mpv key=value]… [--fullscreen]
//!                        [--measure SECONDS | --stress CYCLES | --exercise]
//!
//! The backend is chosen through the environment: `GDK_BACKEND=wayland` (native)
//! or `x11` (XWayland or X11), and optionally `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
//! `DRI_PRIME` selects the GPU. Diagnostics: `S1_DEBUG=1` prints the widget tree and
//! the render calls, `S1_STATIC_SUB=1` freezes the fake subtitle, `S1_NO_INSTALL=1`
//! skips the composition and only dumps the plain Dioxus window.

mod compositor;
mod metrics;
mod player;

use std::path::PathBuf;
use std::sync::OnceLock;

use dioxus::desktop::{Config, DesktopContext, LogicalSize, WindowBuilder, use_window};
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedSender;
use futures_util::StreamExt;

use metrics::Mode;
use player::{Options, UiMsg};

const USAGE: &str = "usage: spike-s1-composition <video> [--hwdec MODE] [--mpv KEY=VALUE]… [--fullscreen] [--measure SECONDS | --stress CYCLES | --exercise]";

#[derive(Debug, Clone)]
struct Args {
    opts: Options,
    mode: Mode,
    fullscreen: bool,
}

static ARGS: OnceLock<Args> = OnceLock::new();

fn parse_args() -> Result<Args, String> {
    let mut video = None;
    let mut hwdec = "auto-safe".to_string();
    let mut mode = Mode::Interactive;
    let mut fullscreen = false;
    let mut extra = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut num = |what: &str| -> Result<u64, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .ok_or(format!("{what}: expected a number"))
        };
        match a.as_str() {
            "--hwdec" => hwdec = it.next().ok_or("--hwdec: missing value")?,
            "--mpv" => {
                let kv = it.next().ok_or("--mpv: missing key=value")?;
                let (k, v) = kv.split_once('=').ok_or("--mpv: expected key=value")?;
                extra.push((k.to_string(), v.to_string()));
            }
            "--measure" => mode = Mode::Measure(num("--measure")?),
            "--stress" => mode = Mode::Stress(num("--stress")? as u32),
            "--exercise" => mode = Mode::Exercise,
            "--fullscreen" => fullscreen = true,
            _ if video.is_none() => video = Some(PathBuf::from(a)),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let video = video.ok_or("missing video path")?;
    Ok(Args {
        opts: Options {
            video,
            hwdec,
            extra,
        },
        mode,
        fullscreen,
    })
}

const CSS: &str = r#"
html, body { margin: 0; height: 100%; overflow: hidden; background: transparent !important;
  font-family: system-ui, "Noto Sans CJK JP", sans-serif; }
#root { position: fixed; inset: 0; outline: none; }
.bar { position: absolute; top: 0; left: 0; right: 0; display: flex; flex-wrap: wrap; gap: 12px;
  align-items: center; padding: 10px 14px; background: rgba(18, 18, 22, .75); color: #eee; font-size: 14px; }
.bar button { font: inherit; padding: 6px 12px; border-radius: 6px; border: 1px solid #666;
  background: #2a2a30; color: #fff; cursor: pointer; }
.bar button:hover { background: #3a3a44; }
.info { opacity: .8; font-variant-numeric: tabular-nums; }
.sub { position: absolute; left: 0; right: 0; bottom: 75px; text-align: center; font-size: 36px;
  color: #fff; text-shadow: 0 0 6px #000, 0 2px 4px #000; font-variant-numeric: tabular-nums; }
.sub .g { cursor: pointer; border-radius: 4px; }
.sub .g:hover { background: rgba(255, 255, 255, .25); }
"#;

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let _ = ARGS.set(args);
    let cfg = Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("Anikoto — spike S1")
                .with_inner_size(LogicalSize::new(1280.0, 720.0)),
        )
        .with_menu(None)
        .with_background_color((0, 0, 0, 0))
        // With `true` (the default) Dioxus 0.7.10 always sets GDK_BACKEND=x11;
        // the backend is chosen by whoever launches the spike instead.
        .with_disable_dma_buf_on_wayland(false)
        .with_custom_head(format!("<style>{CSS}</style>"));
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

fn setup(desktop: &DesktopContext, tx: UnboundedSender<UiMsg>) {
    let Some(args) = ARGS.get().cloned() else {
        return;
    };
    if std::env::var_os("S1_NO_INSTALL").is_some() {
        use dioxus::desktop::tao::platform::unix::WindowExtUnix;
        use gtk::prelude::*;
        let win = desktop.window.gtk_window().clone();
        for ms in [500u64, 1500, 3000] {
            let win = win.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                eprintln!("[s1] --- {ms} ms");
                compositor::dump_tree(win.upcast_ref(), 0);
            });
        }
        glib::timeout_add_local_once(std::time::Duration::from_millis(3500), || {
            std::process::exit(0)
        });
        return;
    }
    let opts = args.opts.clone();
    let tx_attach = tx.clone();
    match compositor::install(desktop, |area| {
        player::attach(area, opts.clone(), tx_attach)
    }) {
        Ok((overlay, area)) => {
            eprintln!(
                "[s1] composition installed: {}",
                compositor::describe(desktop)
            );
            if args.fullscreen {
                desktop.set_fullscreen(true);
            }
            if std::env::var_os("S1_DEBUG").is_some() {
                use dioxus::desktop::tao::platform::unix::WindowExtUnix;
                use gtk::prelude::*;
                let win = desktop.window.gtk_window().clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    compositor::dump_tree(win.upcast_ref(), 0);
                });
            }
            metrics::start(args.mode, desktop.clone(), overlay, area, args.opts, tx);
        }
        Err(e) => {
            let _ = tx.unbounded_send(UiMsg::Error(e));
        }
    }
}

#[component]
fn App() -> Element {
    let desktop = use_window();
    let mut time = use_signal(|| 0.0f64);
    let mut paused = use_signal(|| false);
    let mut fullscreen = use_signal(|| false);
    let mut info = use_signal(String::new);
    let mut clicks = use_signal(|| 0u32);
    let mut keys = use_signal(|| 0u32);
    let mut last = use_signal(|| String::from("—"));

    let hook_desktop = desktop.clone();
    use_hook(move || {
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<UiMsg>();
        // On the next main loop iteration: the webview is already in tao's vbox.
        glib::idle_add_local_once(move || setup(&hook_desktop, tx));
        spawn(async move {
            while let Some(msg) = rx.next().await {
                match msg {
                    UiMsg::Time(t) => {
                        // S1_STATIC_SUB: keep the overlay still, to measure its cost.
                        if std::env::var_os("S1_STATIC_SUB").is_none() {
                            time.set(t);
                        }
                    }
                    UiMsg::Paused(p) => paused.set(p),
                    UiMsg::Info(s) => info.set(s),
                    UiMsg::Error(e) => {
                        eprintln!("[s1] error: {e}");
                        info.set(format!("error: {e}"));
                    }
                }
            }
        });
    });

    // Signal is Copy: the closure can be used in several handlers.
    let mut toggle_fullscreen = move || {
        let fs = !fullscreen();
        fullscreen.set(fs);
        dioxus::desktop::window().set_fullscreen(fs);
    };

    // Fake subtitle: every grapheme is a clickable span and the text changes on
    // every time-pos update (≤ 30 Hz), the worst case for the webview.
    let t = time();
    let text: Vec<char> = format!("字幕のテストです・{t:7.2}秒・クリックしてね")
        .chars()
        .collect();

    rsx! {
        div {
            id: "root",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |e: KeyboardEvent| {
                keys += 1;
                let code = e.code().to_string();
                match code.as_str() {
                    "Space" => player::command(&["cycle", "pause"]),
                    "ArrowLeft" => player::command(&["seek", "-3"]),
                    "ArrowRight" => player::command(&["seek", "3"]),
                    "KeyF" => toggle_fullscreen(),
                    _ => {}
                }
                last.set(format!("key {code}"));
            },
            div { class: "bar",
                button {
                    onclick: move |_| {
                        clicks += 1;
                        last.set("pause button".into());
                        player::command(&["cycle", "pause"]);
                    },
                    if paused() { "▶ Play" } else { "⏸ Pause" }
                }
                button {
                    onclick: move |_| {
                        clicks += 1;
                        last.set("fullscreen button".into());
                        toggle_fullscreen();
                    },
                    "⛶ Fullscreen"
                }
                span { "clicks {clicks} · keys {keys} · last: {last}" }
                span { class: "info", "{info}" }
            }
            div { class: "sub",
                for (i, c) in text.into_iter().enumerate() {
                    span {
                        key: "{i}",
                        class: "g",
                        onclick: move |_| {
                            clicks += 1;
                            last.set(format!("grapheme {i}"));
                        },
                        "{c}"
                    }
                }
            }
        }
    }
}
