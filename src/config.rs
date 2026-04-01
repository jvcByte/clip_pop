// SPDX-License-Identifier: MIT

//! Persistent application configuration via `cosmic-config`.
//! All tuneable constants live here — nothing is hardcoded elsewhere.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use tracing::error;

// ── Identity ──────────────────────────────────────────────────────────────────

/// Application ID in RDNN format. Defined once in `Cargo.toml`
/// under `[package.metadata] app-id` and injected at compile time by `build.rs`.
pub const APP_ID: &str = env!("APP_ID");

// ── History ───────────────────────────────────────────────────────────────────

pub const MIN_HISTORY: usize = 10;
pub const MAX_HISTORY: usize = 500;
pub const DEFAULT_HISTORY: usize = 50;

// ── Clipboard polling ─────────────────────────────────────────────────────────

pub const DEFAULT_POLL_MS: u64 = 500;
pub const MIN_POLL_MS: u64 = 100;
pub const MAX_POLL_MS: u64 = 5_000;

// ── UI ────────────────────────────────────────────────────────────────────────

/// Maximum characters shown in a history list preview before truncation.
pub const DEFAULT_PREVIEW_CHARS: usize = 100;

// ── Window ────────────────────────────────────────────────────────────────────

/// Narrow panel feel — like a clipboard, not a full app window.
pub const WINDOW_WIDTH: f32 = 380.0;
pub const WINDOW_HEIGHT: f32 = 600.0;
pub const WINDOW_MIN_WIDTH: f32 = 320.0;
pub const WINDOW_MIN_HEIGHT: f32 = 400.0;

// ── Persistence ───────────────────────────────────────────────────────────────

/// Sub-directory under `$XDG_DATA_HOME` where app data is stored.
pub const DATA_DIR_NAME: &str = "clip_pop";
/// Filename for the persisted clipboard history.
pub const HISTORY_FILE_NAME: &str = "history.json";
/// Fallback data directory when `$XDG_DATA_HOME` is unavailable.
pub const DATA_DIR_FALLBACK: &str = ".";

// ── Logging ───────────────────────────────────────────────────────────────────

pub const DEFAULT_LOG_LEVEL: &str = "info";

// ── Internal ─────────────────────────────────────────────────────────────────

/// Stable ID for the clipboard watcher subscription.
pub const CLIPBOARD_SUBSCRIPTION_ID: &str = "clip-pop-clipboard-watcher";

// ── Config struct ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    /// Maximum number of clipboard entries to retain (unpinned).
    pub max_history: usize,
    /// Clipboard poll interval in milliseconds.
    pub poll_interval_ms: u64,
    /// Maximum characters shown in a list item preview.
    pub preview_chars: usize,
    /// Move item to top of unpinned section when selected.
    pub move_to_top_on_select: bool,
    /// Pause clipboard recording.
    pub private_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_history: DEFAULT_HISTORY,
            poll_interval_ms: DEFAULT_POLL_MS,
            preview_chars: DEFAULT_PREVIEW_CHARS,
            move_to_top_on_select: true,
            private_mode: false,
        }
    }
}

impl Config {
    /// Clamp all fields to their valid ranges.
    #[must_use]
    pub fn validated(mut self) -> Self {
        self.max_history = self.max_history.clamp(MIN_HISTORY, MAX_HISTORY);
        self.poll_interval_ms = self.poll_interval_ms.clamp(MIN_POLL_MS, MAX_POLL_MS);
        self.preview_chars = self.preview_chars.clamp(20, 500);
        self
    }
}

/// Load config from `cosmic-config`, falling back to defaults on any error.
pub fn load(app_id: &str) -> Config {
    cosmic_config::Config::new(app_id, Config::VERSION)
        .map_err(|e| error!("failed to open config store: {e}"))
        .ok()
        .and_then(|ctx| match Config::get_entry(&ctx) {
            Ok(cfg) => Some(cfg),
            Err((errors, cfg)) => {
                for e in errors {
                    error!("config entry error: {e}");
                }
                Some(cfg)
            }
        })
        .unwrap_or_default()
        .validated()
}
