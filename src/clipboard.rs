// SPDX-License-Identifier: MIT

//! Clipboard polling subscription.
//!
//! Runs in a background async stream, emitting the new clipboard text
//! whenever it changes. Isolates all `arboard` usage from the UI layer.

use crate::config::CLIPBOARD_SUBSCRIPTION_ID;
use arboard::Clipboard;
use cosmic::iced_futures::{self, Subscription};
use cosmic::iced_futures::futures::channel::mpsc::Sender;
use cosmic::iced_futures::futures::SinkExt;
use cosmic::iced_futures::stream;
use std::time::Duration;
use tracing::{error, warn};

/// Returns a [`Subscription`] that polls the system clipboard every
/// `interval_ms` milliseconds and emits `Some(text)` when the content changes,
/// or `None` if the clipboard becomes empty / unreadable.
///
/// Keyed by both the stable ID and the interval value so it restarts
/// automatically if the poll interval changes in config.
pub fn watch(interval_ms: u64) -> Subscription<Option<String>> {
    Subscription::run_with(
        (CLIPBOARD_SUBSCRIPTION_ID, interval_ms),
        |data| {
            let interval_ms = data.1;
            stream::channel(1, move |mut tx: Sender<Option<String>>| async move {
                let mut clipboard = match Clipboard::new() {
                    Ok(cb) => cb,
                    Err(e) => {
                        error!("failed to open clipboard: {e}");
                        std::future::pending::<()>().await;
                        unreachable!()
                    }
                };

                let mut last: Option<String> = None;
                let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));

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
                            break;
                        }
                    }
                }
            })
        },
    )
}

/// Write `text` to the system clipboard. Returns an error string on failure.
pub fn set_text(text: &str) -> Result<(), String> {
    Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .map_err(|e| e.to_string())
}
