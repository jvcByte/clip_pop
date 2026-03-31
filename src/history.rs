// SPDX-License-Identifier: MIT

//! Clipboard history entry types and persistence.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{error, warn};

/// A single clipboard history entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipEntry {
    pub content: String,
    pub timestamp: DateTime<Local>,
}

impl ClipEntry {
    pub fn new(content: String) -> Self {
        Self {
            content,
            timestamp: Local::now(),
        }
    }

    /// Returns a truncated preview of the content for display.
    pub fn preview(&self, max_chars: usize) -> String {
        let trimmed = self.content.trim();
        if trimmed.chars().count() > max_chars {
            let truncated: String = trimmed.chars().take(max_chars).collect();
            format!("{truncated}…")
        } else {
            trimmed.to_owned()
        }
    }

    /// Human-readable relative time string (e.g. "2 minutes ago").
    pub fn relative_time(&self) -> String {
        let now = Local::now();
        let secs = (now - self.timestamp).num_seconds();
        match secs {
            s if s < 60 => "just now".to_owned(),
            s if s < 3600 => format!("{} min ago", s / 60),
            s if s < 86400 => format!("{} hr ago", s / 3600),
            s => format!("{} days ago", s / 86400),
        }
    }
}

/// Manages the in-memory history list and disk persistence.
pub struct HistoryStore {
    entries: Vec<ClipEntry>,
    path: PathBuf,
    max: usize,
}

impl HistoryStore {
    /// Load from disk, or start fresh if the file doesn't exist or is corrupt.
    pub fn load(path: PathBuf, max: usize) -> Self {
        let entries = fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<Vec<ClipEntry>>(&data).ok())
            .unwrap_or_default();

        Self { entries, path, max }
    }

    pub fn entries(&self) -> &[ClipEntry] {
        &self.entries
    }

    /// Add a new entry, deduplicating and trimming to max size.
    /// Returns `true` if the store was modified.
    pub fn push(&mut self, content: String) -> bool {
        if content.trim().is_empty() {
            return false;
        }
        // Remove existing duplicate
        self.entries.retain(|e| e.content != content);
        self.entries.insert(0, ClipEntry::new(content));
        self.entries.truncate(self.max);
        self.persist();
        true
    }

    /// Move an existing entry to the top and re-copy it.
    pub fn promote(&mut self, index: usize) -> Option<&ClipEntry> {
        if index >= self.entries.len() {
            warn!("promote: index {index} out of bounds");
            return None;
        }
        let entry = self.entries.remove(index);
        self.entries.insert(0, entry);
        self.persist();
        self.entries.first()
    }

    /// Remove a single entry by index.
    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.persist();
        }
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.persist();
    }

    /// Update the max size (e.g. after config change).
    pub fn set_max(&mut self, max: usize) {
        self.max = max;
        self.entries.truncate(max);
        self.persist();
    }

    fn persist(&self) {
        match serde_json::to_string_pretty(&self.entries) {
            Ok(data) => {
                if let Some(parent) = self.path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        error!("failed to create history dir: {e}");
                        return;
                    }
                }
                if let Err(e) = fs::write(&self.path, data) {
                    error!("failed to write history: {e}");
                }
            }
            Err(e) => error!("failed to serialize history: {e}"),
        }
    }
}
