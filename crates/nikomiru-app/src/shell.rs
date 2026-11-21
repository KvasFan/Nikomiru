// SPDX-License-Identifier: GPL-3.0-only

//! The event loop: the window system's events and the messages of the other
//! threads drive the interface (`nikomiru_ui::Ui`) and the frames
//! (SPECIFICATION §5.3, ADR 0006).
//!
//! A frame: egui lays out the interface; mpv draws the video into the window's
//! framebuffer; egui paints over it; the buffers swap; mpv is told of the swap.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_glow::EguiGlow;
use egui_winit::accesskit_winit;
use nikomiru_player::Player;
use nikomiru_player::render::VideoRenderer;
use nikomiru_ui::{Inbound, Key, Modifiers, Services, Ui, Waker, Wheel};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::ModifiersState;
#[cfg(not(target_os = "android"))]
use winit::platform::scancode::PhysicalKeyExtScancode;
use winit::window::WindowId;

use crate::platform::dnd::Drops;
use crate::window::GlWindow;

/// What the other threads and the window system send to the event loop.
#[derive(Debug)]
pub enum UserEvent {
    /// A message for the interface.
    Inbound(Inbound),
    /// mpv has a frame to show, or must redraw.
    Video,
    /// egui asks for a frame after this delay.
    Repaint(Duration),
    /// A request of an assistive technology (AccessKit, NFR-A11Y-01).
    AccessKit(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        UserEvent::AccessKit(event)
    }
}

/// The interface's [`Waker`]: hands messages to the event loop.
pub struct ProxyWaker(Mutex<EventLoopProxy<UserEvent>>);

impl std::fmt::Debug for ProxyWaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProxyWaker")
    }
}

impl ProxyWaker {
    /// A waker that sends through `proxy`.
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self(Mutex::new(proxy))
    }
}

impl Waker for ProxyWaker {
    fn wake(&self, message: Inbound) {
        if let Ok(proxy) = self.0.lock() {
            // after the loop has ended, messages are dropped
            let _ = proxy.send_event(UserEvent::Inbound(message));
        }
    }
}

/// The open window: what lives while the event loop runs.
struct Open {
    /// Before `gl`: freed with the GL context current (SPECIFICATION §5.2.2 #7).
    renderer: Option<VideoRenderer>,
    egui: EguiGlow,
    ui: Ui,
    /// Drops on Wayland; before `gl` (it uses the event loop's connection).
    drops: Option<Drops>,
    gl: GlWindow,
}

/// The evdev code of a key (KEY-001 binds keys by it).
#[cfg(not(target_os = "android"))]
fn evdev_code(key: &winit::keyboard::PhysicalKey) -> Option<u32> {
    key.to_scancode()
}

/// The same on Android (D-16), where winit gives no scancode: the keys of a
/// keyboard, a remote and the back button (as Escape).
#[cfg(target_os = "android")]
fn evdev_code(key: &winit::keyboard::PhysicalKey) -> Option<u32> {
    use winit::keyboard::{KeyCode as K, PhysicalKey};
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    let letters = [
        (K::KeyQ, 16),
        (K::KeyW, 17),
        (K::KeyE, 18),
        (K::KeyR, 19),
        (K::KeyT, 20),
        (K::KeyY, 21),
        (K::KeyU, 22),
        (K::KeyI, 23),
        (K::KeyO, 24),
        (K::KeyP, 25),
        (K::KeyA, 30),
        (K::KeyS, 31),
        (K::KeyD, 32),
        (K::KeyF, 33),
        (K::KeyG, 34),
        (K::KeyH, 35),
        (K::KeyJ, 36),
        (K::KeyK, 37),
        (K::KeyL, 38),
        (K::KeyZ, 44),
        (K::KeyX, 45),
        (K::KeyC, 46),
        (K::KeyV, 47),
        (K::KeyB, 48),
        (K::KeyN, 49),
        (K::KeyM, 50),
        (K::Digit1, 2),
        (K::Digit2, 3),
        (K::Digit3, 4),
        (K::Digit4, 5),
        (K::Digit5, 6),
        (K::Digit6, 7),
        (K::Digit7, 8),
        (K::Digit8, 9),
        (K::Digit9, 10),
        (K::Digit0, 11),
        (K::Escape, 1),
        (K::BrowserBack, 1),
        (K::Minus, 12),
        (K::Equal, 13),
        (K::Backspace, 14),
        (K::Tab, 15),
        (K::BracketLeft, 26),
        (K::BracketRight, 27),
        (K::Enter, 28),
        (K::Semicolon, 39),
        (K::Quote, 40),
        (K::Backquote, 41),
        (K::Backslash, 43),
        (K::Comma, 51),
        (K::Period, 52),
        (K::Slash, 53),
        (K::Space, 57),
        (K::F1, 59),
        (K::F2, 60),
        (K::F3, 61),
        (K::F4, 62),
        (K::F5, 63),
        (K::F6, 64),
        (K::F7, 65),
        (K::F8, 66),
        (K::F9, 67),
        (K::F10, 68),
        (K::F11, 87),
        (K::F12, 88),
        (K::Home, 102),
        (K::ArrowUp, 103),
        (K::PageUp, 104),
        (K::ArrowLeft, 105),
        (K::ArrowRight, 106),
        (K::End, 107),
        (K::ArrowDown, 108),
        (K::PageDown, 109),
        (K::Delete, 111),
        (K::MediaPlayPause, 164),
    ];
    letters
        .iter()
        .find(|(k, _)| k == code)
        .map(|&(_, evdev)| evdev)
}

/// The application's event handler.
pub struct Shell<'a> {
    player: &'a Player,
    services: Option<Services>,
    proxy: EventLoopProxy<UserEvent>,
    waker: Arc<dyn Waker>,
    token: Option<String>,
    /// Messages that came before the window: handled once it exists.
    pending: Vec<Inbound>,
    open: Option<Open>,
    modifiers: ModifiersState,
    next_tick: Instant,
    repaint_at: Option<Instant>,
    /// A file of the current drop was taken (LIB-001: only the first counts).
    dropped: bool,
    /// The exit code: 1 when the window could not be created.
    pub code: u8,
}

impl std::fmt::Debug for Shell<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("open", &self.open.is_some())
            .finish_non_exhaustive()
    }
}

fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        ctrl: state.control_key(),
        alt: state.alt_key(),
        shift: state.shift_key(),
    }
}

impl<'a> Shell<'a> {
    /// The handler, before the window exists. `token`: the launcher's
    /// startup-notification token.
    pub fn new(
        player: &'a Player,
        services: Services,
        proxy: EventLoopProxy<UserEvent>,
        waker: Arc<dyn Waker>,
        token: Option<String>,
    ) -> Self {
        Self {
            player,
            services: Some(services),
            proxy,
            waker,
            token,
            pending: Vec::new(),
            open: None,
            modifiers: ModifiersState::empty(),
            next_tick: Instant::now(),
            repaint_at: None,
            dropped: false,
            code: 0,
        }
    }

    fn open_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        services: Services,
    ) -> Result<(), String> {
        let gl = GlWindow::new(event_loop, "Nikomiru", self.token.take())?;
        let mut egui = EguiGlow::new(event_loop, gl.gl.clone(), None, None, true);
        let proxy = Mutex::new(self.proxy.clone());
        egui.egui_ctx.set_request_repaint_callback(move |info| {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(UserEvent::Repaint(info.delay));
            }
        });
        // before the window is shown (accesskit_winit)
        egui.egui_winit
            .init_accesskit(event_loop, &gl.window, self.proxy.clone());
        let mut ui = Ui::new(&egui.egui_ctx, services, self.waker.clone());
        // INS-001: the assistant at the first start; the GUI tests ask for it
        if std::env::var("NIKOMIRU_TEST_NO_SETUP").is_ok_and(|v| !v.is_empty()) {
            tracing::debug!("setup assistant not offered (tests)");
        } else {
            ui.offer_setup();
        }
        let proxy = Mutex::new(self.proxy.clone());
        let renderer = match gl.video_renderer(self.player, move || {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(UserEvent::Video);
            }
        }) {
            Ok(renderer) => {
                ui.renderer_ready();
                Some(renderer)
            }
            Err(e) => {
                tracing::error!("mpv render context: {e}");
                ui.renderer_failed(&e.to_string());
                None
            }
        };
        for message in self.pending.drain(..) {
            ui.handle(message);
        }
        // winit takes no drops on Wayland: the data device is ours (LIB-001)
        let waker = self.waker.clone();
        let drops = Drops::new(
            &gl.window,
            Arc::new(move |files: Vec<PathBuf>| {
                if let Some(first) = files.into_iter().next() {
                    waker.wake(Inbound::Dropped(first));
                }
            }),
        );
        gl.window.set_visible(true);
        gl.window.request_redraw();
        self.open = Some(Open {
            renderer,
            egui,
            ui,
            drops,
            gl,
        });
        Ok(())
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(open) = &mut self.open else {
            return;
        };
        // in the background on Android: nothing to draw into
        if !open.gl.drawable() {
            return;
        }
        let Open {
            renderer,
            egui,
            ui,
            gl,
            ..
        } = open;
        // the window's state for egui (fullscreen, focus, size): EguiGlow does not
        // fill it in itself
        let ctx = egui.egui_ctx.clone();
        let info = egui
            .egui_winit
            .egui_input_mut()
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default();
        egui_winit::update_viewport_info(info, &ctx, &gl.window, false);
        // a phone held upright (Android, D-16): the pages are laid out for at
        // least NARROWEST points across, drawn smaller rather than cut off
        #[cfg(target_os = "android")]
        {
            const NARROWEST: f32 = 560.0;
            let across = gl.window.inner_size().width as f32 / gl.window.scale_factor() as f32;
            let zoom = (across / NARROWEST).min(1.0);
            if (ctx.zoom_factor() - zoom).abs() > 0.01 {
                ctx.set_zoom_factor(zoom);
            }
            // the system bars over the window: egui keeps the pages out of them
            if let Some(insets) =
                crate::platform::android::safe_area(gl.window.inner_size(), ctx.pixels_per_point())
            {
                egui.egui_winit.egui_input_mut().safe_area_insets = Some(insets);
            }
        }
        // keys of the test harness that the window would have given egui
        egui.egui_winit
            .egui_input_mut()
            .events
            .extend(ui.take_injected());
        egui.run(&gl.window, |root| ui.frame(root));
        if let Some(dialog) = ui.take_file_dialog() {
            // Android has no file dialog the app can reach: its own chooser, on
            // the shared storage, which needs "all files access" first (D-16)
            #[cfg(target_os = "android")]
            {
                if !crate::portal::can_read_all_files() {
                    crate::portal::ask_to_read_all_files();
                }
                tracing::info!("file chooser: {}", dialog.title);
                ui.choose_in_app(dialog, PathBuf::from("/storage/emulated/0"));
                gl.window.request_redraw();
            }
            #[cfg(not(target_os = "android"))]
            crate::portal::open_file(dialog, self.waker.clone());
        }
        for url in ui.take_external() {
            crate::portal::open_uri(&url);
        }
        gl.prepare_video();
        match renderer {
            Some(renderer) => {
                let (w, h) = gl.size();
                renderer.render(0, w, h, true);
            }
            None => gl.clear(),
        }
        egui.paint(&gl.window);
        if let Some(area) = ui.take_capture() {
            ui.captured(gl.read_pixels(area));
        }
        if let Err(e) = gl.swap() {
            tracing::warn!("{e}");
        }
        if let Some(renderer) = renderer {
            renderer.report_swap();
        }
        ui.presented();
        if ui.wants_quit() {
            event_loop.exit();
        }
    }

    fn request_redraw(&self) {
        if let Some(open) = &self.open {
            open.gl.window.request_redraw();
        }
    }
}

impl ApplicationHandler<UserEvent> for Shell<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // back from the background (Android): a new surface for the window
        if let Some(open) = &mut self.open {
            if let Err(e) = open.gl.resume() {
                tracing::error!("{e}");
            }
            open.gl.window.request_redraw();
            return;
        }
        let Some(services) = self.services.take() else {
            return;
        };
        if let Err(e) = self.open_window(event_loop, services) {
            // the services (the library included) are dropped here: closed cleanly
            tracing::error!("{e}");
            eprintln!("nikomiru: {e}");
            self.code = 1;
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            self.redraw(event_loop);
            return;
        }
        let mods = self.modifiers;
        let Some(open) = &mut self.open else {
            return;
        };
        match &event {
            WindowEvent::CloseRequested => {
                open.ui.quit();
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                open.gl.resize(*size);
                open.gl.window.request_redraw();
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::MouseWheel { delta, .. } => {
                let pointer = open.egui.egui_ctx.input(|i| i.pointer.latest_pos());
                let wheel = match *delta {
                    MouseScrollDelta::LineDelta(x, y) => Wheel::Lines(if y == 0.0 { x } else { y }),
                    MouseScrollDelta::PixelDelta(p) => {
                        let scale = open.gl.window.scale_factor();
                        Wheel::Points((if p.y == 0.0 { p.x } else { p.y } / scale) as f32)
                    }
                };
                // Alt+wheel over a popup moves through its entries (POP-014)
                if open.ui.wheel(pointer, wheel, mods.alt_key()) {
                    open.gl.window.request_redraw();
                    return;
                }
            }
            WindowEvent::Focused(false) => {
                open.ui.focus_lost();
                open.gl.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                if !self.dropped {
                    self.dropped = true;
                    open.ui.dropped(path);
                    open.gl.window.request_redraw();
                }
                return;
            }
            WindowEvent::KeyboardInput {
                event: key,
                is_synthetic: false,
                ..
            } => {
                if let Some(evdev) = evdev_code(&key.physical_key) {
                    // KEY-004: single keys belong to a text field that has the keyboard
                    let text_focus = open.egui.egui_ctx.egui_wants_keyboard_input();
                    let taken = open.ui.key(
                        Key {
                            evdev,
                            mods: modifiers(mods),
                            pressed: key.state == ElementState::Pressed,
                            repeat: key.repeat,
                        },
                        text_focus,
                    );
                    open.gl.window.request_redraw();
                    if open.ui.wants_quit() {
                        event_loop.exit();
                        return;
                    }
                    // egui sees a key only when a text field has the keyboard, for
                    // an unbound Ctrl chord (copy), or on a page over the video,
                    // whose controls the keyboard drives (NFR-A11Y-01): over the
                    // video Tab and the arrows would move its focus and Space
                    // would press the focused button
                    if !(text_focus || (!taken && (mods.control_key() || open.ui.page_open()))) {
                        return;
                    }
                }
            }
            _ => {}
        }
        let response = open.egui.on_window_event(&open.gl.window, &event);
        if response.repaint {
            open.gl.window.request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Inbound(message) => match &mut self.open {
                Some(open) => {
                    if open.ui.handle(message) {
                        open.gl.window.request_redraw();
                    }
                    if open.ui.wants_quit() {
                        event_loop.exit();
                    }
                }
                // a signal before the window: nothing to save yet
                None if matches!(message, Inbound::Quit) => event_loop.exit(),
                None => self.pending.push(message),
            },
            UserEvent::Video => self.request_redraw(),
            UserEvent::Repaint(delay) => {
                if delay.is_zero() {
                    self.request_redraw();
                } else {
                    let at = Instant::now() + delay;
                    self.repaint_at = Some(self.repaint_at.map_or(at, |r| r.min(at)));
                }
            }
            UserEvent::AccessKit(event) => {
                let Some(open) = &mut self.open else {
                    return;
                };
                match event.window_event {
                    accesskit_winit::WindowEvent::InitialTreeRequested => {
                        open.egui.egui_ctx.enable_accesskit();
                        open.gl.window.request_redraw();
                    }
                    accesskit_winit::WindowEvent::ActionRequested(request) => {
                        open.egui.egui_winit.on_accesskit_action_request(request);
                        open.gl.window.request_redraw();
                    }
                    accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                        open.egui.egui_ctx.disable_accesskit();
                    }
                }
            }
        }
    }

    /// To the background (Android): the window's surface goes away.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(open) = &mut self.open {
            open.gl.suspend();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.dropped = false;
        let now = Instant::now();
        if let Some(open) = &mut self.open {
            if let Some(drops) = &mut open.drops {
                drops.dispatch();
            }
            if now >= self.next_tick {
                if open.ui.tick() {
                    open.gl.window.request_redraw();
                }
                self.next_tick = now + open.ui.tick_interval();
            }
            if self.repaint_at.is_some_and(|at| now >= at) {
                self.repaint_at = None;
                open.gl.window.request_redraw();
            }
            if open.ui.wants_quit() {
                event_loop.exit();
                return;
            }
        }
        let wake = self
            .repaint_at
            .map_or(self.next_tick, |at| at.min(self.next_tick));
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(open) = self.open.take() {
            let Open {
                renderer,
                mut egui,
                ui,
                drops,
                gl,
            } = open;
            // SPECIFICATION §5.2.2 #7: mpv's render context first, with the GL
            // context current; then egui's painter; then the session (it saves
            // the position, joins its workers and closes the library); the window last
            drop(renderer);
            egui.destroy();
            drop(ui);
            drop(drops);
            drop(gl);
        }
        // a window that never opened: the services must be closed too (T-REL-001)
        self.services = None;
    }
}
