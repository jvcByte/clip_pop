// SPDX-License-Identifier: MIT

//! Persistent application configuration via `cosmic-config`.
//! All tuneable constants live here — nothing is hardcoded elsewhere.

use std::sync::atomic::AtomicBool;

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use tracing::error;

// ── Identity ──────────────────────────────────────────────────────────────────

pub const APP_ID: &str = env!("APP_ID");

// ── History ───────────────────────────────────────────────────────────────────

pub const MIN_HISTORY: usize = 10;
pub const MAX_HISTORY: usize = 500;
pub const DEFAULT_HISTORY: usize = 50;

// ── UI ────────────────────────────────────────────────────────────────────────

pub const DEFAULT_PREVIEW_CHARS: usize = 100;
pub const MIN_PREVIEW_CHARS: usize = 20;
pub const MAX_PREVIEW_CHARS: usize = 500;

// ── Window ────────────────────────────────────────────────────────────────────

pub const WINDOW_WIDTH: f32 = 380.0;
pub const WINDOW_HEIGHT: f32 = 600.0;
pub const WINDOW_MIN_WIDTH: f32 = 320.0;
pub const WINDOW_MIN_HEIGHT: f32 = 400.0;

// ── Persistence ───────────────────────────────────────────────────────────────

pub const DATA_DIR_NAME: &str = "clip_pop";
pub const DB_FILE_NAME: &str = "history.db";

/// Autostart desktop file installed to `~/.config/autostart/`.
pub const AUTOSTART_DESKTOP_FILE: &str = "com.github.jvcByte.clip_pop.desktop";

/// Returns the path to the user autostart directory entry.
pub fn autostart_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("autostart")
        .join(AUTOSTART_DESKTOP_FILE)
}

// ── Logging ───────────────────────────────────────────────────────────────────

pub const DEFAULT_LOG_LEVEL: &str = "info";

// ── Internal ─────────────────────────────────────────────────────────────────

pub const CLIPBOARD_SUBSCRIPTION_ID: &str = "clip-pop-clipboard-watcher";

// ── Private mode atomic ───────────────────────────────────────────────────────

/// Shared atomic so the clipboard watcher thread can check private mode
/// without acquiring a lock on the full app state.
pub static PRIVATE_MODE: AtomicBool = AtomicBool::new(false);

// ── Config struct ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    /// Maximum number of clipboard entries to retain (unpinned).
    pub max_history: usize,
    /// Maximum characters shown in a list item preview.
    pub preview_chars: usize,
    /// Move item to top of history when selected.
    pub move_to_top_on_select: bool,
    /// Pause clipboard recording.
    pub private_mode: bool,
    /// User-defined preferred MIME type patterns (regex strings).
    pub preferred_mime_types: Vec<String>,
    /// Auto-delete unpinned entries older than this many days. None = never.
    pub entry_lifetime_days: Option<u64>,
    /// Launch Clip Pop automatically on login.
    pub launch_on_login: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_history: DEFAULT_HISTORY,
            preview_chars: DEFAULT_PREVIEW_CHARS,
            move_to_top_on_select: true,
            private_mode: false,
            preferred_mime_types: Vec::new(),
            entry_lifetime_days: Some(30),
            launch_on_login: false,
        }
    }
}

impl Config {
    #[must_use]
    pub fn validated(mut self) -> Self {
        self.max_history = self.max_history.clamp(MIN_HISTORY, MAX_HISTORY);
        self.preview_chars = self.preview_chars.clamp(MIN_PREVIEW_CHARS, MAX_PREVIEW_CHARS);
        self
    }
}

/// Load config from `cosmic-config`, falling back to defaults on any error.
pub fn load(app_id: &str) -> (cosmic_config::Config, Config) {
    let ctx = cosmic_config::Config::new(app_id, Config::VERSION)
        .map_err(|e| error!("failed to open config store: {e}"))
        .unwrap_or_else(|_| {
            cosmic_config::Config::system(app_id, Config::VERSION)
                .unwrap_or_else(|_| panic!("failed to create config context"))
        });

    let config = match Config::get_entry(&ctx) {
        Ok(cfg) => cfg,
        Err((errors, cfg)) => {
            for e in errors {
                error!("config entry error: {e}");
            }
            cfg
        }
    }
    .validated();

    (ctx, config)
}
