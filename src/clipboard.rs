// SPDX-License-Identifier: MIT

//! Clipboard subscription.
//!
//! Tries to use the event-driven `zwlr_data_control_v1` watcher first.
//! Falls back to `wl-clipboard-rs` polling at 100ms if the protocol is
//! unavailable on the compositor.

use std::io::Read;
use std::sync::atomic;
use std::time::Duration;

use crate::clipboard_watcher::{self, ClipboardContent};
use crate::config::{CLIPBOARD_SUBSCRIPTION_ID, PRIVATE_MODE};
use cosmic::iced_futures::Subscription;
use cosmic::iced_futures::futures::channel::mpsc::Sender;
use cosmic::iced_futures::futures::SinkExt;
use cosmic::iced_futures::stream;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use wl_clipboard_rs::paste::{
    ClipboardType, Error as PasteError, MimeType, Seat, get_contents, get_mime_types,
};

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

const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    Text(String),
    Image { data: Vec<u8>, mime_type: String },
    Cleared,
    Unavailable,
}

pub fn watch() -> Subscription<ClipboardEvent> {
    Subscription::run_with(CLIPBOARD_SUBSCRIPTION_ID, |_| {
        stream::channel(8, move |mut tx: Sender<ClipboardEvent>| async move {
            let (thread_tx, mut thread_rx) = mpsc::channel::<ClipboardEvent>(8);

            tokio::task::spawn_blocking(move || {
                // Try event-driven watcher first
                match clipboard_watcher::Watcher::init() {
                    Ok(mut watcher) => {
                        info!("clipboard: using event-driven zwlr_data_control watcher");
                        loop {
                            if PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                                std::thread::sleep(Duration::from_millis(200));
                                continue;
                            }
                            match watcher.next() {
                                Some(ClipboardContent::Text(text)) => {
                                    let _ = thread_tx.blocking_send(ClipboardEvent::Text(text));
                                }
                                Some(ClipboardContent::Image { data, mime_type }) => {
                                    let _ = thread_tx.blocking_send(ClipboardEvent::Image { data, mime_type });
                                }
                                Some(ClipboardContent::Cleared) => {
                                    let _ = thread_tx.blocking_send(ClipboardEvent::Cleared);
                                }
                                None => {
                                    error!("clipboard watcher connection lost");
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("event-driven watcher unavailable ({e}), falling back to polling");
                        poll_loop(thread_tx);
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

/// Fallback polling loop using wl-clipboard-rs.
fn poll_loop(tx: mpsc::Sender<ClipboardEvent>) {
    info!("clipboard: using wl-clipboard-rs polling fallback (100ms)");
    let mut last_hash: Option<u64> = None;

    loop {
        std::thread::sleep(POLL_INTERVAL);

        if PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
            continue;
        }

        let mime_types = match get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
            Ok(types) => types,
            Err(PasteError::ClipboardEmpty) => {
                if last_hash.is_some() {
                    last_hash = None;
                    let _ = tx.blocking_send(ClipboardEvent::Cleared);
                }
                continue;
            }
            Err(PasteError::MissingProtocol { name, version }) => {
                error!("clipboard protocol unavailable: {name} v{version}");
                let _ = tx.blocking_send(ClipboardEvent::Unavailable);
                break;
            }
            Err(e) => {
                warn!("clipboard mime types error: {e}");
                continue;
            }
        };

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

        let Some(mime) = chosen else { continue };

        let (mut reader, actual_mime) = match get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific(mime),
        ) {
            Ok(r) => r,
            Err(PasteError::ClipboardEmpty) => {
                last_hash = None;
                let _ = tx.blocking_send(ClipboardEvent::Cleared);
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

        if tx.blocking_send(event).is_err() {
            break;
        }
    }
}

pub fn set_text(text: &str) -> Result<(), String> {
    use wl_clipboard_rs::copy::{MimeType as CopyMime, Options, Source};
    Options::new()
        .copy(
            Source::Bytes(text.as_bytes().into()),
            CopyMime::Specific("text/plain;charset=utf-8".into()),
        )
        .map_err(|e| e.to_string())
}

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
