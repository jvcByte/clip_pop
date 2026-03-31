// SPDX-License-Identifier: MIT

//! Clipboard polling subscription.
//!
//! Runs in a background async stream, emitting the new clipboard text
//! whenever it changes. Isolates all `arboard` usage from the UI layer.

use crate::config::CLIPBOARD_SUBSCRIPTION_ID;
use arboard::Clipboard;
use cosmic::iced::Subscription;
use cosmic::iced_futures;
use futures_util::SinkExt;
use std::time::Duration;
use tracing::{error, warn};

/// Returns a [`Subscription`] that polls the system clipboard every
/// `interval_ms` milliseconds and emits `Some(text)` when the content changes,
/// or `None` if the clipboard becomes empty / unreadable.
pub fn watch(interval_ms: u64) -> Subscription<Option<String>> {
    Subscription::run_with_id(
        CLIPBOARD_SUBSCRIPTION_ID,
        iced_futures::stream::channel(1, move |mut tx| async move {
            let mut clipboard = match Clipboard::new() {
                Ok(cb) => cb,
                Err(e) => {
                    error!("failed to open clipboard: {e}");
                    // Park the stream — nothing we can do without a clipboard handle.
                    std::future::pending::<()>().await;
                    unreachable!()
                }
            };

            let mut last: Option<String> = None;
            let mut interval =
                tokio::time::interval(Duration::from_millis(interval_ms));

            loop {
                interval.tick().await;

                let current = match clipboard.get_text() {
                    Ok(text) if !text.is_empty() => Some(text),
                    Ok(_) => None,
                    Err(arboard::Error::ContentNotAvailable) => None,
                    Err(e) => {
                        warn!("clipboard read error: {e}");
                        None
                    }
                };

                if current != last {
                    last = current.clone();
                    if tx.send(current).await.is_err() {
                        // Receiver dropped — app is shutting down.
                        break;
                    }
                }
            }
        }),
    )
}

/// Write `text` to the system clipboard. Returns an error string on failure.
pub fn set_text(text: &str) -> Result<(), String> {
    Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .map_err(|e| e.to_string())
}
