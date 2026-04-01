// SPDX-License-Identifier: MIT

//! Event-driven clipboard watcher using the zwlr_data_control_v1 Wayland protocol.
//!
//! Blocks on `blocking_dispatch` until the compositor fires a `Selection` event,
//! then reads the clipboard content. Zero polling — fires instantly on copy.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;

use os_pipe::pipe;
use tracing::warn;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_registry::{self, WlRegistry},
        wl_seat::{self, WlSeat},
    },
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
};

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("failed to connect to Wayland display: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    #[error("zwlr_data_control_manager_v1 protocol not supported by compositor")]
    ProtocolUnsupported,
    #[error("Wayland dispatch error: {0}")]
    Dispatch(#[from] wayland_client::DispatchError),
    #[error("no seats available")]
    NoSeats,
}

/// MIME types we care about, in priority order.
const PREFERRED_MIME: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/bmp",
    "image/webp",
];

pub enum ClipboardContent {
    Text(String),
    Image { data: Vec<u8>, mime_type: String },
    Cleared,
}

// ── Wayland state ─────────────────────────────────────────────────────────────

struct AppState {
    manager: Option<ZwlrDataControlManagerV1>,
    seats: Vec<WlSeat>,
    /// MIME types accumulating for each pending offer (keyed by object id as u32).
    pending_offers: HashMap<u32, Vec<String>>,
    /// The current clipboard selection: (offer, mime_types).
    selection: Option<(ZwlrDataControlOfferV1, Vec<String>)>,
    got_selection: bool,
}

impl AppState {
    fn new() -> Self {
        Self {
            manager: None,
            seats: Vec::new(),
            pending_offers: HashMap::new(),
            selection: None,
            got_selection: false,
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for AppState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "zwlr_data_control_manager_v1" => {
                    let mgr: ZwlrDataControlManagerV1 =
                        registry.bind(name, version.min(2), qh, ());
                    state.manager = Some(mgr);
                }
                "wl_seat" => {
                    let seat: WlSeat = registry.bind(name, version.min(8), qh, ());
                    state.seats.push(seat);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for AppState {
    fn event(
        _: &mut Self, _: &ZwlrDataControlManagerV1,
        _: wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<WlSeat, ()> for AppState {
    fn event(
        _: &mut Self, _: &WlSeat,
        _: wl_seat::Event, _: &(),
        _: &Connection, _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _device: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                let key = id.id().protocol_id();
                state.pending_offers.insert(key, Vec::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                if let Some(offer_obj) = id {
                    let key = offer_obj.id().protocol_id();
                    let mimes = state.pending_offers.remove(&key).unwrap_or_default();
                    state.selection = Some((offer_obj, mimes));
                } else {
                    state.selection = None;
                }
                state.got_selection = true;
            }
            zwlr_data_control_device_v1::Event::Finished => {}
            _ => {}
        }
    }

    event_created_child!(AppState, ZwlrDataControlDeviceV1, [
        0 => (ZwlrDataControlOfferV1, ())
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for AppState {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            let key = offer.id().protocol_id();
            if let Some(mimes) = state.pending_offers.get_mut(&key) {
                mimes.push(mime_type);
            }
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub struct Watcher {
    queue: EventQueue<AppState>,
    state: AppState,
}

impl Watcher {
    pub fn init() -> Result<Self, WatcherError> {
        let conn = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<AppState>(&conn)
            .map_err(|_| WatcherError::ProtocolUnsupported)?;

        let qh = queue.handle();
        let mut state = AppState::new();

        // Bind globals via registry
        for global in globals.contents().clone_list() {
            match global.interface.as_str() {
                "zwlr_data_control_manager_v1" => {
                    let mgr: ZwlrDataControlManagerV1 =
                        globals.registry().bind(global.name, global.version.min(2), &qh, ());
                    state.manager = Some(mgr);
                }
                "wl_seat" => {
                    let seat: WlSeat =
                        globals.registry().bind(global.name, global.version.min(8), &qh, ());
                    state.seats.push(seat);
                }
                _ => {}
            }
        }

        queue.roundtrip(&mut state)
            .map_err(WatcherError::Dispatch)?;

        if state.manager.is_none() {
            return Err(WatcherError::ProtocolUnsupported);
        }
        if state.seats.is_empty() {
            return Err(WatcherError::NoSeats);
        }

        // Create data devices for all seats
        let manager = state.manager.as_ref().unwrap();
        let devices: Vec<ZwlrDataControlDeviceV1> = state
            .seats
            .iter()
            .map(|seat| manager.get_data_device(seat, &qh, ()))
            .collect();

        // Store devices to keep them alive
        for device in devices {
            let _ = device; // devices are kept alive by the queue
        }

        // Get initial selection state
        queue.roundtrip(&mut state)
            .map_err(WatcherError::Dispatch)?;

        Ok(Self { queue, state })
    }

    /// Block until the clipboard changes, then return the new content.
    /// Returns `None` if the Wayland connection was lost.
    pub fn next(&mut self) -> Option<ClipboardContent> {
        self.state.got_selection = false;

        loop {
            if self.queue.blocking_dispatch(&mut self.state).is_err() {
                return None;
            }
            if self.state.got_selection {
                break;
            }
        }

        let Some((offer, mime_types)) = self.state.selection.take() else {
            return Some(ClipboardContent::Cleared);
        };

        let chosen = PREFERRED_MIME
            .iter()
            .find(|&&m| mime_types.iter().any(|t| t == m))
            .copied()
            .or_else(|| {
                mime_types
                    .iter()
                    .find(|m| m.starts_with("text/") || m.starts_with("image/"))
                    .map(|s| s.as_str())
            });

        let Some(mime) = chosen else {
            return Some(ClipboardContent::Cleared);
        };

        let (read, write) = match pipe() {
            Ok(p) => p,
            Err(e) => {
                warn!("pipe creation failed: {e}");
                return Some(ClipboardContent::Cleared);
            }
        };

        offer.receive(mime.to_owned(), write.as_fd());
        drop(write);

        if let Err(e) = self.queue.flush() {
            warn!("flush failed: {e}");
        }

        let mut content = Vec::new();
        if let Err(e) = { let mut r = read; r.read_to_end(&mut content).map(|_| ()) } {
            warn!("pipe read failed: {e}");
            return Some(ClipboardContent::Cleared);
        }

        if content.is_empty() {
            return Some(ClipboardContent::Cleared);
        }

        let actual_mime = mime.to_owned();
        if actual_mime.starts_with("image/") {
            Some(ClipboardContent::Image { data: content, mime_type: actual_mime })
        } else {
            match String::from_utf8(content) {
                Ok(text) if !text.trim().is_empty() => Some(ClipboardContent::Text(text)),
                Ok(_) => Some(ClipboardContent::Cleared),
                Err(e) => {
                    warn!("utf8 decode: {e}");
                    Some(ClipboardContent::Cleared)
                }
            }
        }
    }
}
