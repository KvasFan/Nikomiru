// SPDX-License-Identifier: GPL-3.0-only

//! Spike S5 with iced (0.14, wgpu on its OpenGL backend): the video is a
//! `shader` widget into which mpv renders (see `video`), under S1q's overlay
//! (a bar with buttons, a fake subtitle whose graphemes are clickable and
//! change 30 times per second) and, with `--popups`, Nikomiru's dictionary
//! popup. iced follows the Elm architecture: every message runs `update`,
//! then `view` builds the widget tree again.

mod popup;
mod video;

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, mouse_area, row, shader, stack, text};
use iced::{
    Background, Color, Element, Length, Padding, Size, Subscription, Task, keyboard, window,
};
use s5_common::mpv::{Event, Mpv};
use s5_common::popup::Content;
use s5_common::probe::{FrameProbe, PopupCommand, PopupMeasure, VideoMeasure};
use s5_common::{Args, Mode};
use serde_json::json;

fn main() -> iced::Result {
    // nothing else runs yet
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // libmpv renders only through OpenGL
        unsafe { std::env::set_var("WGPU_BACKEND", "gl") };
    }
    if std::env::var_os("S5_MPV_OPTIONS").is_none() {
        // in wgpu's GL context, VA-API without copies gives garbled frames
        // (the dmabuf import succeeds, the planes read as zeros); with a
        // copy back to memory they are right
        unsafe { std::env::set_var("S5_MPV_OPTIONS", "hwdec=auto-copy-safe") };
    }
    let args = Args::parse();
    let fullscreen = args.fullscreen;
    iced::application(move || App::new(args.clone()), App::update, App::view)
        .title("Nikomiru — spike S5 (iced)")
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1280.0, 720.0),
            fullscreen,
            ..Default::default()
        })
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Poll,
    Key(keyboard::Event),
    TogglePause,
    ToggleFullscreen,
    Grapheme(usize),
    Resized(Size),
}

struct App {
    args: Args,
    video: Arc<video::Video>,
    loaded: bool,
    paused: bool,
    graphemes: Vec<String>,
    clicks: u32,
    keys: u32,
    last: String,
    fullscreen: bool,
    size: Size,
    measure: Option<VideoMeasure>,
    popups: Vec<(String, Content)>,
    popup_measure: Option<PopupMeasure>,
    /// The popup shown: index, generation, views built so far.
    shown: Option<(usize, u64)>,
    probe: Arc<FrameProbe>,
    /// Time spent in `view` while a popup is shown, after its first frame.
    view_ms: RefCell<(u64, Vec<f64>)>,
}

impl App {
    fn new(args: Args) -> App {
        // the 30 Hz poll reads mpv's events, as S1q throttles time-pos
        let mpv = Mpv::new(|| {});
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
            video: Arc::new(video::Video {
                mpv,
                stats: Arc::default(),
                ready: AtomicBool::new(false),
                dirty: Arc::default(),
                playing: AtomicBool::new(false),
            }),
            loaded: false,
            paused: false,
            graphemes: s5_common::subtitle(0.0),
            clicks: 0,
            keys: 0,
            last: "—".into(),
            size: Size::new(1280.0, 720.0),
            measure,
            popups,
            popup_measure,
            shown: None,
            probe: Arc::default(),
            view_ms: RefCell::default(),
        }
    }

    fn mpv(&self) -> &Mpv {
        &self.video.mpv
    }

    fn set_fullscreen(&mut self, on: bool) -> Task<Message> {
        self.fullscreen = on;
        let mode = if on {
            window::Mode::Fullscreen
        } else {
            window::Mode::Windowed
        };
        window::latest().and_then(move |id| window::set_mode(id, mode))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Poll => return self.poll(),
            Message::Key(keyboard::Event::KeyPressed { key, .. }) => {
                self.keys += 1;
                self.last = format!("key {key:?}");
                use keyboard::key::Named;
                match key.as_ref() {
                    keyboard::Key::Named(Named::Space) => self.mpv().command(&["cycle", "pause"]),
                    keyboard::Key::Named(Named::ArrowLeft) => self.mpv().command(&["seek", "-3"]),
                    keyboard::Key::Named(Named::ArrowRight) => self.mpv().command(&["seek", "3"]),
                    keyboard::Key::Character("f") => return self.set_fullscreen(!self.fullscreen),
                    keyboard::Key::Character("q") | keyboard::Key::Named(Named::Escape) => {
                        return iced::exit();
                    }
                    _ => {}
                }
            }
            Message::Key(_) => {}
            Message::TogglePause => {
                self.clicks += 1;
                self.last = "pause button".into();
                self.mpv().command(&["cycle", "pause"]);
            }
            Message::ToggleFullscreen => {
                self.clicks += 1;
                self.last = "fullscreen button".into();
                return self.set_fullscreen(!self.fullscreen);
            }
            Message::Grapheme(i) => {
                self.clicks += 1;
                self.last = format!("grapheme {i}");
            }
            Message::Resized(size) => self.size = size,
        }
        Task::none()
    }

    fn poll(&mut self) -> Task<Message> {
        let mut time = None;
        for e in self.mpv().poll() {
            match e {
                Event::TimePos(t) => time = Some(t),
                Event::Pause(p) => {
                    self.paused = p;
                    self.video.playing.store(!p, Ordering::Release);
                }
                Event::FileLoaded if self.popup_measure.is_some() => {
                    // as the app does while a popup is open
                    self.mpv().command(&["set", "pause", "yes"]);
                }
                _ => {}
            }
        }
        // the poll runs every 33 ms: at most 30 changes per second, as S1q
        // throttles time-pos (checking the elapsed time here as well would
        // skip the ticks that come a little early)
        if let Some(t) = time {
            self.graphemes = s5_common::subtitle(t);
        }
        if !self.loaded && self.video.ready.load(Ordering::Acquire) {
            // only once the render context exists (ADR 0003, finding 1)
            self.mpv().command(&["loadfile", &self.args.video]);
            self.loaded = true;
            self.video.playing.store(true, Ordering::Release);
        }
        if let Some(m) = &mut self.measure {
            let extra = json!({
                "toolkit": "iced",
                "video": self.args.video,
                "window_logical": [self.size.width, self.size.height],
            });
            if let Some(report) = m.tick(&self.video.mpv, &self.video.stats, extra) {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
                std::process::exit(0);
            }
        }
        if let Some(m) = &mut self.popup_measure
            && self.loaded
        {
            match m.tick(&self.probe) {
                Ok(Some(PopupCommand::Show { index, generation })) => {
                    self.shown = Some((index, generation));
                    m.applied();
                }
                Ok(Some(PopupCommand::Hide)) => self.shown = None,
                Ok(None) => {}
                Err(mut report) => {
                    let mut t = self.view_ms.borrow().1.clone();
                    t.sort_by(f64::total_cmp);
                    let at = |q: f64| {
                        t.get(((t.len() as f64 * q) as usize).min(t.len().saturating_sub(1)))
                            .copied()
                    };
                    report["toolkit"] = json!("iced");
                    report["window_logical"] = json!([self.size.width, self.size.height]);
                    report["view_ms_per_rebuild_with_popup"] =
                        json!({ "p50": at(0.5), "p95": at(0.95), "rebuilds": t.len() });
                    println!("{}", serde_json::to_string_pretty(&report).unwrap());
                    std::process::exit(0);
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let started = Instant::now();
        let video = shader(video::Program(Arc::clone(&self.video)))
            .width(Length::Fill)
            .height(Length::Fill);
        let play = if self.paused { "▶ Play" } else { "⏸ Pause" };
        let bar = container(
            row![
                button(text(play))
                    .width(120)
                    .height(30)
                    .on_press(Message::TogglePause),
                button(text("⛶ Fullscreen"))
                    .width(140)
                    .height(30)
                    .on_press(Message::ToggleFullscreen),
                text(format!(
                    "clicks {} · keys {} · last: {}",
                    self.clicks, self.keys, self.last
                ))
                .color(Color::from_rgb8(0xee, 0xee, 0xee)),
            ]
            .spacing(12)
            .align_y(Vertical::Center),
        )
        .padding([7, 14])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba8(0x12, 0x12, 0x16, 0.75))),
            ..Default::default()
        });
        let px = (36.0 * self.size.height / 1080.0).max(18.0);
        let subtitle = row(self.graphemes.iter().enumerate().map(|(i, g)| {
            // white with a black outline, as S1q's Text.Outline
            let mut layers: Vec<Element<'_, Message>> =
                [(0.0, 1.5), (3.0, 1.5), (1.5, 0.0), (1.5, 3.0)]
                    .into_iter()
                    .map(|(x, y)| {
                        container(text(g).size(px).color(Color::BLACK))
                            .padding(Padding {
                                left: x,
                                top: y,
                                right: 3.0 - x,
                                bottom: 3.0 - y,
                            })
                            .into()
                    })
                    .collect();
            layers.push(
                container(text(g).size(px).color(Color::WHITE))
                    .padding(1.5)
                    .into(),
            );
            mouse_area(stack(layers))
                .on_press(Message::Grapheme(i))
                .into()
        }));
        let mut layers: Vec<Element<'_, Message>> = vec![
            video.into(),
            column![bar].into(),
            container(subtitle)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Bottom)
                .padding(Padding::default().bottom(75.0 * self.size.height / 1080.0))
                .into(),
        ];
        if let Some((index, generation)) = self.shown {
            let width = 560f32.min(self.size.width - 16.0);
            let popup = popup::view(
                &self.popups[index].1,
                width,
                self.size.height * 0.7,
                &self.probe,
                generation,
            );
            layers.push(
                container(popup)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Bottom)
                    .padding(Padding::default().bottom(self.size.height * 0.28 + 8.0))
                    .into(),
            );
            let mut v = self.view_ms.borrow_mut();
            if v.0 == generation {
                v.1.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            v.0 = generation;
        }
        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_millis(33)).map(|_| Message::Poll),
            keyboard::listen().map(Message::Key),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
        ])
    }
}
