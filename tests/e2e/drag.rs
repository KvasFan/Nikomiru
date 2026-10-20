// SPDX-License-Identifier: GPL-3.0-only

//! A Wayland client that drags files onto Nikomiru in the off-screen KWin, as
//! a file manager would: a small layer-shell surface in a corner, a button
//! press on it through KWin's fake input, a drag of a `text/uri-list` with the
//! copy action, a move over the window and the release.

use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::WlDataOffer;
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop, event_created_child,
};
use wayland_protocols_plasma::fake_input::client::org_kde_kwin_fake_input::OrgKdeKwinFakeInput;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{
    Layer, ZwlrLayerShellV1,
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1,
};

const SIZE: i32 = 96;
const BTN_LEFT: u32 = 0x110;

#[derive(Default)]
struct State {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    seat: Option<WlSeat>,
    manager: Option<WlDataDeviceManager>,
    layers: Option<ZwlrLayerShellV1>,
    fake: Option<OrgKdeKwinFakeInput>,
    configured: Option<u32>,
    entered: bool,
    pressed: Option<u32>,
    uris: String,
    /// What the drop came to: the data was sent, the target finished, or
    /// the drag was cancelled.
    sent: bool,
    finished: bool,
    cancelled: bool,
    accepted: Option<String>,
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(5), qh, ()))
                }
                "wl_data_device_manager" => {
                    state.manager = Some(registry.bind(name, version.min(3), qh, ()))
                }
                "zwlr_layer_shell_v1" => state.layers = Some(registry.bind(name, 1, qh, ())),
                "org_kde_kwin_fake_input" => {
                    state.fake = Some(registry.bind(name, version.min(4), qh, ()))
                }
                _ => {}
            }
        }
    }
}

delegate_noop!(State: ignore WlCompositor);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ignore WlDataDeviceManager);
delegate_noop!(State: ignore WlDataOffer);
delegate_noop!(State: ignore ZwlrLayerShellV1);
delegate_noop!(State: ignore OrgKdeKwinFakeInput);

impl Dispatch<ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut Self,
        surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            surface.ack_configure(serial);
            state.configured = Some(serial);
        }
    }
}

impl Dispatch<WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { .. } => state.entered = true,
            wl_pointer::Event::Button {
                serial,
                button: BTN_LEFT,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => state.pressed = Some(serial),
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlDataDevice,
        _: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Target { mime_type } => state.accepted = mime_type,
            wl_data_source::Event::Send { mime_type, fd } => {
                if mime_type == "text/uri-list" {
                    let mut pipe = std::fs::File::from(fd);
                    let _ = pipe.write_all(state.uris.as_bytes());
                }
                state.sent = true;
            }
            wl_data_source::Event::DndFinished => state.finished = true,
            wl_data_source::Event::Cancelled => state.cancelled = true,
            _ => {}
        }
    }
}

/// How a drag ended, seen from its source.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The target accepted this type.
    pub accepted: Option<String>,
    /// It asked for the data.
    pub sent: bool,
    /// It said the drop was done.
    pub finished: bool,
    /// The drag was cancelled.
    pub cancelled: bool,
}

/// A client of the desktop's compositor that can drag files.
pub struct DragSource {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
}

impl DragSource {
    /// Connects to the compositor's socket.
    pub fn connect(socket: &Path) -> Self {
        let stream = UnixStream::connect(socket).expect("the compositor's socket");
        let conn = Connection::from_socket(stream).expect("a Wayland connection");
        let mut queue = conn.new_event_queue();
        conn.display().get_registry(&queue.handle(), ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        for (what, there) in [
            ("wl_compositor", state.compositor.is_some()),
            ("wl_shm", state.shm.is_some()),
            ("wl_seat", state.seat.is_some()),
            ("wl_data_device_manager", state.manager.is_some()),
            ("zwlr_layer_shell_v1", state.layers.is_some()),
            ("org_kde_kwin_fake_input", state.fake.is_some()),
        ] {
            assert!(there, "the compositor has no {what}");
        }
        Self { conn, queue, state }
    }

    /// Dispatches until `done` or the timeout.
    fn until(&mut self, what: &str, done: impl Fn(&State) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.state) {
            assert!(Instant::now() < deadline, "drag: no {what}");
            self.queue.roundtrip(&mut self.state).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Drags `files` from the top-left corner and drops them at `to`
    /// (window coordinates, which are the screen's for a window at the
    /// origin). Returns how the drag ended.
    pub fn drag(&mut self, files: &[&str], to: (f64, f64)) -> Outcome {
        let qh = self.queue.handle();
        // what the last drag saw
        let s = &mut self.state;
        (s.configured, s.entered, s.pressed, s.accepted) = (None, false, None, None);
        (s.sent, s.finished, s.cancelled) = (false, false, false);
        self.state.uris = files
            .iter()
            .map(|f| {
                let encoded: String = f
                    .bytes()
                    .map(|b| match b {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' => {
                            char::from(b).to_string()
                        }
                        b => format!("%{b:02X}"),
                    })
                    .collect();
                format!("file://{encoded}\r\n")
            })
            .collect();
        let s = &self.state;
        let (compositor, shm, seat, manager, layers, fake) = (
            s.compositor.clone().unwrap(),
            s.shm.clone().unwrap(),
            s.seat.clone().unwrap(),
            s.manager.clone().unwrap(),
            s.layers.clone().unwrap(),
            s.fake.clone().unwrap(),
        );
        fake.authenticate("nikomiru-e2e".into(), "drag and drop test".into());

        // a grey square in the top-left corner, above every window
        let surface = compositor.create_surface(&qh, ());
        let layer = layers.get_layer_surface(
            &surface,
            None,
            Layer::Overlay,
            "nikomiru-e2e".into(),
            &qh,
            (),
        );
        layer.set_size(SIZE as u32, SIZE as u32);
        layer.set_anchor(Anchor::Top | Anchor::Left);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        surface.commit();
        self.until("layer configure", |s| s.configured.is_some());
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&vec![0x80; (SIZE * SIZE * 4) as usize])
            .unwrap();
        let pool = shm.create_pool(file.as_fd(), SIZE * SIZE * 4, &qh, ());
        let buffer = pool.create_buffer(0, SIZE, SIZE, SIZE * 4, wl_shm::Format::Argb8888, &qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.damage(0, 0, SIZE, SIZE);
        surface.commit();
        let pointer = seat.get_pointer(&qh, ());
        let device = manager.get_data_device(&seat, &qh, ());
        self.queue.roundtrip(&mut self.state).unwrap();

        // press on the square: the serial of the press starts the drag
        let half = f64::from(SIZE) / 2.0;
        fake.pointer_motion_absolute(half, half);
        self.until("pointer enter", |s| s.entered);
        fake.button(BTN_LEFT, 1);
        self.until("button press", |s| s.pressed.is_some());
        let source = manager.create_data_source(&qh, ());
        source.offer("text/uri-list".into());
        source.set_actions(DndAction::Copy | DndAction::Move);
        device.start_drag(
            Some(&source),
            &surface,
            None,
            self.state.pressed.unwrap_or(0),
        );
        self.queue.roundtrip(&mut self.state).unwrap();

        // over the window in a few steps, then release
        for step in 1..=5 {
            let t = f64::from(step) / 5.0;
            fake.pointer_motion_absolute(half + (to.0 - half) * t, half + (to.1 - half) * t);
            self.queue.roundtrip(&mut self.state).unwrap();
            std::thread::sleep(Duration::from_millis(30));
        }
        self.until("the target's answer", |s| s.accepted.is_some());
        fake.button(BTN_LEFT, 0);
        self.until("the end of the drag", |s| s.finished || s.cancelled);

        let outcome = Outcome {
            accepted: self.state.accepted.clone(),
            sent: self.state.sent,
            finished: self.state.finished,
            cancelled: self.state.cancelled,
        };
        source.destroy();
        pointer.release();
        layer.destroy();
        surface.destroy();
        buffer.destroy();
        pool.destroy();
        let _ = self.conn.flush();
        outcome
    }
}
