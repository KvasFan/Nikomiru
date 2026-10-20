// SPDX-License-Identifier: GPL-3.0-only

//! Drag and drop on Wayland (LIB-001, DIC-001): winit 0.30 takes no drops
//! there, so Nikomiru listens to the seat's data device itself, on winit's own
//! connection with an event queue of its own, dispatched from the event loop.
//! Only `text/uri-list` offers are taken, and only with the copy action: a
//! move would let the source delete its files. The files go to the callback.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::Arc;

use nikomiru_ui::session::url_to_path;
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child,
};
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::window::Window;

/// The type of a list of files (RFC 2483).
const URI_LIST: &str = "text/uri-list";
/// A dropped list longer than this is not read further.
const MAX_LIST: u64 = 1 << 20;

/// What is called with the files of a drop, on a thread of its own.
pub type OnDrop = Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>;

/// The files of a `text/uri-list`: its `file://` addresses, comments and
/// other schemes left out.
pub fn uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(|l| l.trim_end_matches('\r').trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| l.starts_with("file://"))
        .map(url_to_path)
        .collect()
}

#[derive(Debug, Default)]
struct Offer {
    mime_types: Vec<String>,
    /// The action the compositor chose (version 3).
    action: Option<DndAction>,
}

struct State {
    conn: Connection,
    manager: Option<(WlDataDeviceManager, u32)>,
    seat: Option<WlSeat>,
    device: Option<WlDataDevice>,
    offers: HashMap<ObjectId, Offer>,
    /// The offer over the window, and whether it holds files.
    current: Option<(WlDataOffer, bool)>,
    on_drop: OnDrop,
}

impl State {
    fn forget(&mut self, offer: &WlDataOffer) {
        self.offers.remove(&offer.id());
        offer.destroy();
    }

    /// Reads the files of a dropped offer on a thread, then tells the source
    /// the drop is done.
    fn take(&mut self, offer: WlDataOffer) {
        let info = self.offers.remove(&offer.id()).unwrap_or_default();
        let version = self.manager.as_ref().map_or(1, |(_, v)| *v);
        let finish = version >= 3 && info.action.is_some_and(|a| !a.is_empty());
        let (mut reader, writer) = match std::io::pipe() {
            Ok(pipe) => pipe,
            Err(e) => {
                tracing::warn!("drop: {e}");
                offer.destroy();
                return;
            }
        };
        offer.receive(URI_LIST.into(), writer.as_fd());
        drop(writer);
        let _ = self.conn.flush();
        let conn = self.conn.clone();
        let on_drop = Arc::clone(&self.on_drop);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-drop".into())
            .spawn(move || {
                let mut text = String::new();
                let read = reader.by_ref().take(MAX_LIST).read_to_string(&mut text);
                if finish {
                    offer.finish();
                }
                offer.destroy();
                let _ = conn.flush();
                match read {
                    Ok(_) => {
                        let files = uri_list(&text);
                        if files.is_empty() {
                            tracing::debug!("drop without files");
                        } else {
                            on_drop(files);
                        }
                    }
                    Err(e) => tracing::warn!("drop: {e}"),
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("drop thread: {e}");
        }
    }

    fn make_device(&mut self, qh: &QueueHandle<Self>) {
        if self.device.is_none()
            && let (Some((manager, _)), Some(seat)) = (&self.manager, &self.seat)
        {
            self.device = Some(manager.get_data_device(seat, qh, ()));
        }
    }
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
                "wl_data_device_manager" if state.manager.is_none() => {
                    let version = version.min(3);
                    state.manager = Some((registry.bind(name, version, qh, ()), version));
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, 1, qh, ()));
                }
                _ => {}
            }
            state.make_device(qh);
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: <WlDataDeviceManager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.offers.insert(id.id(), Offer::default());
            }
            wl_data_device::Event::Enter { serial, id, .. } => {
                if let Some((old, _)) = state.current.take() {
                    state.forget(&old);
                }
                let Some(offer) = id else {
                    return;
                };
                let files = state
                    .offers
                    .get(&offer.id())
                    .is_some_and(|o| o.mime_types.iter().any(|m| m == URI_LIST));
                if files {
                    offer.accept(serial, Some(URI_LIST.into()));
                    if state.manager.as_ref().is_some_and(|(_, v)| *v >= 3) {
                        offer.set_actions(DndAction::Copy, DndAction::Copy);
                    }
                } else {
                    offer.accept(serial, None);
                }
                state.current = Some((offer, files));
            }
            wl_data_device::Event::Leave => {
                if let Some((offer, _)) = state.current.take() {
                    state.forget(&offer);
                }
            }
            wl_data_device::Event::Drop => match state.current.take() {
                Some((offer, true)) => state.take(offer),
                Some((offer, false)) => state.forget(&offer),
                None => {}
            },
            // the clipboard is egui's business
            wl_data_device::Event::Selection { id: Some(offer) } => state.forget(&offer),
            _ => {}
        }
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(info) = state.offers.get_mut(&offer.id()) else {
            return;
        };
        match event {
            wl_data_offer::Event::Offer { mime_type } => info.mime_types.push(mime_type),
            wl_data_offer::Event::Action {
                dnd_action: WEnum::Value(action),
            } => info.action = Some(action),
            _ => {}
        }
    }
}

/// The files dropped on the window, on Wayland.
pub struct Drops {
    queue: EventQueue<State>,
    state: State,
}

impl std::fmt::Debug for Drops {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drops").finish_non_exhaustive()
    }
}

impl Drops {
    /// Listens for drops on `window`'s Wayland connection; `None` on X11.
    /// The value must be dropped before the event loop, whose connection it
    /// uses (the shell drops it with the window).
    pub fn new(window: &Window, on_drop: OnDrop) -> Option<Self> {
        let RawDisplayHandle::Wayland(handle) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: the `wl_display` is the event loop's connection, alive until
        // the loop ends; `Drops` lives in the shell's state, dropped in
        // `exiting` before that. The backend is a guest: it never closes it.
        let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State {
            conn,
            manager: None,
            seat: None,
            device: None,
            offers: HashMap::new(),
            current: None,
            on_drop,
        };
        if let Err(e) = queue.roundtrip(&mut state) {
            tracing::warn!("drag and drop: {e}");
            return None;
        }
        if state.device.is_none() {
            tracing::warn!("drag and drop: the compositor has no data device or seat");
            return None;
        }
        Some(Self { queue, state })
    }

    /// Handles the events of the data device that the event loop read.
    pub fn dispatch(&mut self) {
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            tracing::warn!("drag and drop: {e}");
        }
        let _ = self.state.conn.flush();
    }
}

impl Drop for Drops {
    fn drop(&mut self) {
        if let Some((offer, _)) = self.state.current.take() {
            offer.destroy();
        }
        if let Some(device) = self.state.device.take()
            && device.version() >= 2
        {
            device.release();
        }
        let _ = self.state.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_list_gives_its_files() {
        let list = "# from the file manager\r\nfile:///v/My%20Show/ep%2001.mkv\r\nhttps://example.org/x.mkv\r\n\r\nfile:///d/jmdict.zip\n";
        assert_eq!(
            uri_list(list),
            [
                PathBuf::from("/v/My Show/ep 01.mkv"),
                PathBuf::from("/d/jmdict.zip")
            ]
        );
        assert!(uri_list("").is_empty());
    }
}
