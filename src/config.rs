// SPDX-License-Identifier: MIT

//! Persistent application configuration via `cosmic-config`.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use tracing::error;

/// Minimum and maximum allowed values for `max_history`.
pub const MIN_HISTORY: usize = 10;
pub const MAX_HISTORY: usize = 500;
pub const DEFAULT_HISTORY: usize = 50;

/// Clipboard poll interval in milliseconds.
pub const DEFAULT_POLL_MS: u64 = 500;
pub const MIN_POLL_MS: u64 = 100;
pub const MAX_POLL_MS: u64 = 5000;

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    /// Maximum number of clipboard entries to retain.
    pub max_history: usize,
    /// Clipboard poll interval in milliseconds.
    pub poll_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_history: DEFAULT_HISTORY,
            poll_interval_ms: DEFAULT_POLL_MS,
        }
    }
}

impl Config {
    /// Clamp all fields to their valid ranges.
    pub fn validated(mut self) -> Self {
        self.max_history = self.max_history.clamp(MIN_HISTORY, MAX_HISTORY);
        self.poll_interval_ms = self.poll_interval_ms.clamp(MIN_POLL_MS, MAX_POLL_MS);
        self
    }
}

/// Load config from `cosmic-config`, falling back to defaults on any error.
pub fn load(app_id: &str) -> Config {
    cosmic_config::Config::new(app_id, Config::VERSION)
        .map_err(|e| {
            error!("failed to open config store: {e}");
        })
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
