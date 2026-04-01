// SPDX-License-Identifier: MIT

//! Clipboard watcher using wl-clipboard-rs.
//!
//! wl-clipboard-rs uses the zwlr_data_control / ext_data_control Wayland protocol
//! directly — no X11, no focus requirement. We run it in a spawn_blocking thread
//! that polls at a short interval (100ms) since wl-clipboard-rs doesn't expose a
//! blocking "wait for change" API. This is still far better than arboard's 500ms
//! polling because we use the correct Wayland protocol.

use std::io::Read;
use std::sync::atomic;
use std::time::Duration;

use crate::config::{CLIPBOARD_SUBSCRIPTION_ID, PRIVATE_MODE};
use cosmic::iced_futures::Subscription;
use cosmic::iced_futures::futures::channel::mpsc::Sender;
use cosmic::iced_futures::futures::SinkExt;
use cosmic::iced_futures::stream;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::mpsc;
use tracing::{error, warn};
use wl_clipboard_rs::paste::{
    ClipboardType, Error as PasteError, MimeType, Seat, get_contents, get_mime_types,
};

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

/// Poll interval for the clipboard watcher thread.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    Text(String),
    Image { data: Vec<u8>, mime_type: String },
    Cleared,
    /// Protocol not available on this compositor.
    Unavailable,
}

pub fn watch() -> Subscription<ClipboardEvent> {
    Subscription::run_with(CLIPBOARD_SUBSCRIPTION_ID, |_| {
        stream::channel(8, move |mut tx: Sender<ClipboardEvent>| async move {
            let (thread_tx, mut thread_rx) = mpsc::channel::<ClipboardEvent>(8);

            tokio::task::spawn_blocking(move || {
                let mut last_hash: Option<u64> = None;

                loop {
                    std::thread::sleep(POLL_INTERVAL);

                    if PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                        continue;
                    }

                    // Get available MIME types
                    let mime_types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
                        Ok(types) => types,
                        Err(PasteError::ClipboardEmpty) => {
                            if last_hash.is_some() {
                                last_hash = None;
                                let _ = thread_tx.blocking_send(ClipboardEvent::Cleared);
                            }
                            continue;
                        }
                        Err(PasteError::MissingProtocol { name, version }) => {
                            error!("clipboard protocol unavailable: {name} v{version}");
                            let _ = thread_tx.blocking_send(ClipboardEvent::Unavailable);
                            break;
                        }
                        Err(e) => {
                            warn!("clipboard mime types error: {e}");
                            continue;
                        }
                    };

                    // Pick the best available MIME type
                    let chosen = PREFERRED_MIME
                        .iter()
                        .find(|&&m| mime_types.contains(m))
                        .copied()
                        .or_else(|| {
                            mime_types
                                .iter()
                                .find(|m| m.starts_with("text/") || m.starts_with("image/"))
                                .map(|s| s.as_str())
                        });

                    let Some(mime) = chosen else {
                        continue;
                    };

                    // Read the content
                    let (mut reader, actual_mime) = match get_contents(
                        ClipboardType::Regular,
                        Seat::Unspecified,
                        MimeType::Specific(mime),
                    ) {
                        Ok(r) => r,
                        Err(PasteError::ClipboardEmpty) => {
                            last_hash = None;
                            let _ = thread_tx.blocking_send(ClipboardEvent::Cleared);
                            continue;
                        }
                        Err(e) => {
                            warn!("clipboard read error ({mime}): {e}");
                            continue;
                        }
                    };

                    let mut content = Vec::new();
                    if let Err(e) = reader.read_to_end(&mut content) {
                        warn!("clipboard pipe read error: {e}");
                        continue;
                    }

                    if content.is_empty() {
                        continue;
                    }

                    let hash = fx_hash(&content);
                    if last_hash == Some(hash) {
                        continue;
                    }
                    last_hash = Some(hash);

                    let event = if actual_mime.starts_with("image/") {
                        ClipboardEvent::Image { data: content, mime_type: actual_mime }
                    } else {
                        match String::from_utf8(content) {
                            Ok(text) if !text.trim().is_empty() => ClipboardEvent::Text(text),
                            Ok(_) => continue,
                            Err(e) => {
                                warn!("clipboard text decode error: {e}");
                                continue;
                            }
                        }
                    };

                    if thread_tx.blocking_send(event).is_err() {
                        break;
                    }
                }
            });

            while let Some(event) = thread_rx.recv().await {
                let is_unavailable = matches!(event, ClipboardEvent::Unavailable);
                if tx.send(event).await.is_err() {
                    break;
                }
                if is_unavailable {
                    break;
                }
            }
        })
    })
}

/// Write text to the clipboard.
pub fn set_text(text: &str) -> Result<(), String> {
    use wl_clipboard_rs::copy::{MimeType as CopyMime, Options, Source};
    Options::new()
        .copy(
            Source::Bytes(text.as_bytes().into()),
            CopyMime::Specific("text/plain;charset=utf-8".into()),
        )
        .map_err(|e| e.to_string())
}

/// Write image bytes to the clipboard.
pub fn set_image(data: &[u8], mime_type: &str) -> Result<(), String> {
    use wl_clipboard_rs::copy::{MimeType as CopyMime, Options, Source};
    Options::new()
        .copy(
            Source::Bytes(data.into()),
            CopyMime::Specific(mime_type.into()),
        )
        .map_err(|e| e.to_string())
}

pub fn fx_hash(data: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    data.hash(&mut h);
    h.finish()
}
