// SPDX-License-Identifier: MIT

//! Clipboard polling subscription.
//! Isolates all arboard usage from the UI layer.

use crate::config::CLIPBOARD_SUBSCRIPTION_ID;
use arboard::Clipboard;
use cosmic::iced_futures::Subscription;
use cosmic::iced_futures::futures::channel::mpsc::Sender;
use cosmic::iced_futures::futures::SinkExt;
use cosmic::iced_futures::stream;
use std::time::Duration;
use tracing::{error, warn};

/// What changed on the clipboard.
#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    Text(String),
    Image { rgba: Vec<u8>, width: u32, height: u32 },
    Cleared,
}

pub fn watch(interval_ms: u64) -> Subscription<ClipboardEvent> {
    Subscription::run_with(
        (CLIPBOARD_SUBSCRIPTION_ID, interval_ms),
        |data| {
            let interval_ms = data.1;
            stream::channel(1, move |mut tx: Sender<ClipboardEvent>| async move {
                let mut clipboard = match Clipboard::new() {
                    Ok(cb) => cb,
                    Err(e) => {
                        error!("failed to open clipboard: {e}");
                        return;
                    }
                };

                let mut last_text: Option<String> = None;
                let mut last_img_hash: Option<u64> = None;
                let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));

                loop {
                    interval.tick().await;

                    // Try text first
                    match clipboard.get_text() {
                        Ok(text) if !text.is_empty() => {
                            if last_text.as_deref() != Some(text.as_str()) {
                                last_text = Some(text.clone());
                                last_img_hash = None;
                                if tx.send(ClipboardEvent::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                            continue;
                        }
                        Ok(_) => {}
                        Err(arboard::Error::ContentNotAvailable) => {}
                        Err(e) => warn!("clipboard text read error: {e}"),
                    }

                    // Try image
                    match clipboard.get_image() {
                        Ok(img) => {
                            // Simple hash to detect changes without storing full RGBA
                            let hash = simple_hash(&img.bytes);
                            if last_img_hash != Some(hash) {
                                last_img_hash = Some(hash);
                                last_text = None;
                                let event = ClipboardEvent::Image {
                                    rgba: img.bytes.into_owned(),
                                    width: img.width as u32,
                                    height: img.height as u32,
                                };
                                if tx.send(event).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(arboard::Error::ContentNotAvailable) => {
                            if last_text.is_some() || last_img_hash.is_some() {
                                last_text = None;
                                last_img_hash = None;
                                if tx.send(ClipboardEvent::Cleared).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => warn!("clipboard image read error: {e}"),
                    }
                }
            })
        },
    )
}

/// Write text to the system clipboard.
pub fn set_text(text: &str) -> Result<(), String> {
    Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .map_err(|e| e.to_string())
}

/// Write image (RGBA bytes) to the system clipboard.
pub fn set_image(rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    let img_data = arboard::ImageData {
        bytes: std::borrow::Cow::Borrowed(rgba),
        width: width as usize,
        height: height as usize,
    };
    Clipboard::new()
        .and_then(|mut cb| cb.set_image(img_data))
        .map_err(|e| e.to_string())
}

fn simple_hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // Hash a sample of bytes for speed (first 1KB + last 1KB + length)
    let len = data.len();
    len.hash(&mut h);
    data[..len.min(1024)].hash(&mut h);
    if len > 1024 {
        data[len.saturating_sub(1024)..].hash(&mut h);
    }
    h.finish()
}
