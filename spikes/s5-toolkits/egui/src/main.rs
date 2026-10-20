// SPDX-License-Identifier: GPL-3.0-only

//! Spike S5 with egui (eframe on OpenGL): mpv draws the video straight into
//! the window's framebuffer from a paint callback, under everything else;
//! egui then draws S1q's overlay (a bar with buttons, a fake subtitle whose
//! graphemes are clickable and change 30 times per second) and, with
//! `--popups`, Nikomiru's dictionary popup. egui is immediate mode: the
//! whole UI is rebuilt and repainted on every frame.

mod popup;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Id, Sense, vec2};
use eframe::egui_glow;
use s5_common::mpv::{Event, Mpv, RenderStats, Renderer};
use s5_common::popup::Content;
use s5_common::probe::{FrameProbe, PopupCommand, PopupMeasure, VideoMeasure};
use s5_common::{Args, Mode};
use serde_json::json;

fn main() -> eframe::Result {
    let args = Args::parse();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Nikomiru — spike S5 (egui)")
            .with_inner_size([1280.0, 720.0])
            .with_fullscreen(args.fullscreen),
        ..Default::default()
    };
    eframe::run_native(
        "spike-s5-egui",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, args)))),
    )
}

/// Noto Sans CJK for every text, as the system font Qt uses; a bold family
/// for the glossaries' bold spans.
fn fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let dir = "/usr/share/fonts/noto-cjk";
    if let (Ok(regular), Ok(bold)) = (
        std::fs::read(format!("{dir}/NotoSansCJK-Regular.ttc")),
        std::fs::read(format!("{dir}/NotoSansCJK-Bold.ttc")),
    ) {
        defs.font_data
            .insert("cjk".into(), Arc::new(egui::FontData::from_owned(regular)));
        defs.font_data.insert(
            "cjk-bold".into(),
            Arc::new(egui::FontData::from_owned(bold)),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            defs.families
                .entry(family)
                .or_default()
                .insert(0, "cjk".into());
        }
        defs.families.insert(
            FontFamily::Name("bold".into()),
            vec!["cjk-bold".into(), "cjk".into()],
        );
    } else {
        eprintln!("[s5] Noto Sans CJK not found in {dir}");
        defs.families.insert(
            FontFamily::Name("bold".into()),
            defs.families[&FontFamily::Proportional].clone(),
        );
    }
    ctx.set_fonts(defs);
}

struct App {
    args: Args,
    mpv: Arc<Mpv>,
    stats: Arc<RenderStats>,
    /// Made in the first paint callback, where the GL context is current.
    renderer: Arc<Mutex<Option<Renderer>>>,
    loaded: bool,
    paused: bool,
    graphemes: Vec<String>,
    last_subtitle: Instant,
    clicks: u32,
    keys: u32,
    last: String,
    fullscreen: bool,
    measure: Option<VideoMeasure>,
    popups: Vec<(String, Content)>,
    popup_measure: Option<PopupMeasure>,
    /// The popup shown: index, generation, frames drawn so far.
    shown: Option<(usize, u64, u32)>,
    probe: Arc<FrameProbe>,
    /// Time spent in `ui()` on frames that show a popup, after its first.
    popup_frame_ms: Vec<f64>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> App {
        fonts(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let mpv = Mpv::new(move || ctx.request_repaint());
        let (measure, popups, popup_measure) = match &args.mode {
            Mode::Measure(seconds) => (Some(VideoMeasure::new(*seconds)), Vec::new(), None),
            Mode::Popups(path) => {
                let popups = s5_common::popup::load(path).expect("popups file");
                let names = popups.iter().map(|(q, _)| q.clone()).collect();
                (None, popups, Some(PopupMeasure::new(names, 3)))
            }
            Mode::Interactive => (None, Vec::new(), None),
        };
        App {
            fullscreen: args.fullscreen,
            args,
            mpv,
            stats: Arc::default(),
            renderer: Arc::default(),
            loaded: false,
            paused: false,
            graphemes: s5_common::subtitle(0.0),
            last_subtitle: Instant::now(),
            clicks: 0,
            keys: 0,
            last: "—".into(),
            measure,
            popups,
            popup_measure,
            shown: None,
            probe: Arc::default(),
            popup_frame_ms: Vec::new(),
        }
    }

    fn mpv_events(&mut self) {
        for e in self.mpv.poll() {
            match e {
                // 30 times per second, as S1q throttles time-pos
                Event::TimePos(t)
                    if self.last_subtitle.elapsed() >= std::time::Duration::from_millis(33) =>
                {
                    self.graphemes = s5_common::subtitle(t);
                    self.last_subtitle = Instant::now();
                }
                Event::Pause(p) => self.paused = p,
                Event::FileLoaded if self.popup_measure.is_some() => {
                    // as the app does while a popup is open
                    self.mpv.command(&["set", "pause", "yes"]);
                }
                _ => {}
            }
        }
    }

    fn keys(&mut self, ctx: &egui::Context) {
        let pressed: Vec<egui::Key> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => Some(*key),
                    _ => None,
                })
                .collect()
        });
        for key in pressed {
            self.keys += 1;
            self.last = format!("key {key:?}");
            match key {
                egui::Key::Space => self.mpv.command(&["cycle", "pause"]),
                egui::Key::ArrowLeft => self.mpv.command(&["seek", "-3"]),
                egui::Key::ArrowRight => self.mpv.command(&["seek", "3"]),
                egui::Key::F => self.toggle_fullscreen(ctx),
                egui::Key::Q | egui::Key::Escape => std::process::exit(0),
                _ => {}
            }
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    /// The video, under everything: mpv renders into the default framebuffer.
    fn video(&self, ui: &egui::Ui) {
        let renderer = Arc::clone(&self.renderer);
        let mpv = Arc::clone(&self.mpv);
        let stats = Arc::clone(&self.stats);
        let ctx = ui.ctx().clone();
        let callback = egui_glow::CallbackFn::new(move |info, _painter| {
            let mut slot = renderer.lock().unwrap();
            if slot.is_none() {
                let ctx = ctx.clone();
                match unsafe {
                    Renderer::new(Arc::clone(&mpv), Arc::clone(&stats), move || {
                        ctx.request_repaint()
                    })
                } {
                    Ok(r) => *slot = Some(r),
                    Err(e) => eprintln!("[s5] mpv render context: {e}"),
                }
            }
            if let Some(r) = slot.as_ref() {
                // about the previous frame: eframe has no hook after the swap
                if stats.renders.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                    r.report_swap();
                }
                let [w, h] = info.screen_size_px;
                unsafe { r.render(0, w as i32, h as i32, true) };
            }
        });
        ui.painter().add(egui::PaintCallback {
            rect: ui.ctx().content_rect(),
            callback: Arc::new(callback),
        });
    }

    fn overlay(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("bar"))
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(0x12, 0x12, 0x16, 0xbf))
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .show(ui, |ui| {
                        ui.set_width(screen.width() - 28.0);
                        ui.horizontal(|ui| {
                            let play = if self.paused { "▶ Play" } else { "⏸ Pause" };
                            if ui
                                .add_sized([120.0, 30.0], egui::Button::new(play))
                                .clicked()
                            {
                                self.clicks += 1;
                                self.last = "pause button".into();
                                self.mpv.command(&["cycle", "pause"]);
                            }
                            if ui
                                .add_sized([140.0, 30.0], egui::Button::new("⛶ Fullscreen"))
                                .clicked()
                            {
                                self.clicks += 1;
                                self.last = "fullscreen button".into();
                                self.toggle_fullscreen(ctx);
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "clicks {} · keys {} · last: {}",
                                    self.clicks, self.keys, self.last
                                ))
                                .color(Color32::from_gray(0xee)),
                            );
                        });
                    });
            });
        let px = (36.0 * screen.height() / 1080.0).max(18.0);
        egui::Area::new(Id::unique("subtitle"))
            .anchor(
                Align2::CENTER_BOTTOM,
                vec2(0.0, -75.0 * screen.height() / 1080.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, g) in self.graphemes.iter().enumerate() {
                        // white with a black outline, as S1q's Text.Outline
                        let galley = ui.painter().layout_no_wrap(
                            g.clone(),
                            FontId::proportional(px),
                            Color32::WHITE,
                        );
                        let (rect, response) =
                            ui.allocate_exact_size(galley.size(), Sense::click());
                        for d in [
                            vec2(-1.5, 0.0),
                            vec2(1.5, 0.0),
                            vec2(0.0, -1.5),
                            vec2(0.0, 1.5),
                        ] {
                            ui.painter().galley_with_override_text_color(
                                rect.min + d,
                                galley.clone(),
                                Color32::BLACK,
                            );
                        }
                        ui.painter().galley(rect.min, galley, Color32::WHITE);
                        if response.clicked() {
                            self.clicks += 1;
                            self.last = format!("grapheme {i}");
                        }
                    }
                });
            });
    }

    fn measurements(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        if let Some(m) = &mut self.measure {
            let extra = json!({
                "toolkit": "egui",
                "video": self.args.video,
                "window_logical": [screen.width(), screen.height()],
                "pixels_per_point": ctx.pixels_per_point(),
            });
            if let Some(report) = m.tick(&self.mpv, &self.stats, extra) {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
                std::process::exit(0);
            }
        }
        if let Some(m) = &mut self.popup_measure {
            if !self.loaded {
                return;
            }
            match m.tick(&self.probe) {
                Ok(Some(PopupCommand::Show { index, generation })) => {
                    self.shown = Some((index, generation, 0));
                    m.applied();
                }
                Ok(Some(PopupCommand::Hide)) => self.shown = None,
                Ok(None) => {}
                Err(mut report) => {
                    let mut t = self.popup_frame_ms.clone();
                    t.sort_by(f64::total_cmp);
                    let at = |q: f64| {
                        t.get(((t.len() as f64 * q) as usize).min(t.len().saturating_sub(1)))
                            .copied()
                    };
                    report["toolkit"] = json!("egui");
                    report["window_logical"] = json!([screen.width(), screen.height()]);
                    report["ui_ms_per_frame_with_popup"] =
                        json!({ "p50": at(0.5), "p95": at(0.95), "frames": t.len() });
                    println!("{}", serde_json::to_string_pretty(&report).unwrap());
                    std::process::exit(0);
                }
            }
            // keep ticking while the video is paused
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        self.mpv_events();
        if !self.loaded && self.renderer.lock().unwrap().is_some() {
            // only once the render context exists (ADR 0003, finding 1)
            self.mpv.command(&["loadfile", &self.args.video]);
            self.loaded = true;
        }
        self.keys(&ctx);
        self.measurements(&ctx);
        self.video(ui);
        self.overlay(&ctx);
        if let Some((index, generation, frames)) = &mut self.shown {
            if *frames == 0 {
                // a new area is sized in an invisible pass and drawn in the
                // next frame; a second pass in this frame draws it at once
                ctx.request_discard("new popup");
            }
            popup::show(&ctx, &self.popups[*index].1, &self.probe, *generation);
            if *frames > 0 {
                self.popup_frame_ms
                    .push(started.elapsed().as_secs_f64() * 1000.0);
            }
            *frames += 1;
        }
    }
}
