// SPDX-License-Identifier: MIT

//! Clipboard history entry types and persistence.

use crate::fl;
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
    /// Pinned entries stay at the top and are never auto-removed.
    pub pinned: bool,
}

impl ClipEntry {
    pub fn new(content: String) -> Self {
        Self {
            content,
            timestamp: Local::now(),
            pinned: false,
        }
    }

    /// Single-line truncated preview, whitespace collapsed.
    pub fn preview(&self, max_chars: usize) -> String {
        let collapsed: String = self
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if collapsed.chars().count() > max_chars {
            let truncated: String = collapsed.chars().take(max_chars).collect();
            format!("{truncated}…")
        } else {
            collapsed
        }
    }

    pub fn age_secs(&self) -> i64 {
        (Local::now() - self.timestamp).num_seconds()
    }

    /// Localised human-readable relative time string.
    pub fn relative_time_i18n(&self) -> String {
        let secs = self.age_secs();
        match secs {
            s if s < 60 => fl!("time-just-now"),
            s if s < 3_600 => fl!("time-minutes-ago", count = (s / 60i64)),
            s if s < 86_400 => fl!("time-hours-ago", count = (s / 3_600i64)),
            s => fl!("time-days-ago", count = (s / 86_400i64)),
        }
    }
}

/// Manages the in-memory history list and disk persistence.
///
/// Layout: pinned entries first (in insertion order), then unpinned (newest first).
pub struct HistoryStore {
    entries: Vec<ClipEntry>,
    path: PathBuf,
    max: usize,
}

impl HistoryStore {
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

    /// Add a new entry. Deduplicates, respects max (unpinned only).
    /// Returns `true` if modified.
    pub fn push(&mut self, content: String) -> bool {
        if content.trim().is_empty() {
            return false;
        }
        // Remove existing duplicate (unpinned only — don't disturb pins)
        self.entries.retain(|e| e.pinned || e.content != content);

        // Insert after last pinned entry
        let insert_at = self.entries.iter().position(|e| !e.pinned).unwrap_or(self.entries.len());
        self.entries.insert(insert_at, ClipEntry::new(content));

        // Trim unpinned entries to max
        self.trim_unpinned();
        self.persist();
        true
    }

    /// Set item as active (move to top of unpinned section).
    pub fn promote(&mut self, index: usize) -> Option<&ClipEntry> {
        if index >= self.entries.len() {
            warn!("promote: index {index} out of bounds");
            return None;
        }
        if self.entries[index].pinned {
            // Pinned items don't move
            return self.entries.get(index);
        }
        let entry = self.entries.remove(index);
        let insert_at = self.entries.iter().position(|e| !e.pinned).unwrap_or(self.entries.len());
        self.entries.insert(insert_at, entry);
        self.persist();
        self.entries.get(insert_at)
    }

    /// Toggle pin state on an entry.
    pub fn toggle_pin(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        let entry = self.entries.remove(index);
        let was_pinned = entry.pinned;
        let mut entry = entry;
        entry.pinned = !was_pinned;

        if entry.pinned {
            // Move to end of pinned section
            let insert_at = self.entries.iter().rposition(|e| e.pinned).map_or(0, |p| p + 1);
            self.entries.insert(insert_at, entry);
        } else {
            // Move to top of unpinned section
            let insert_at = self.entries.iter().position(|e| !e.pinned).unwrap_or(self.entries.len());
            self.entries.insert(insert_at, entry);
        }
        self.persist();
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.persist();
        }
    }

    /// Clear all unpinned entries.
    pub fn clear_unpinned(&mut self) {
        self.entries.retain(|e| e.pinned);
        self.persist();
    }

    pub fn set_max(&mut self, max: usize) {
        self.max = max;
        self.trim_unpinned();
        self.persist();
    }

    fn trim_unpinned(&mut self) {
        let mut unpinned_count = 0usize;
        self.entries.retain(|e| {
            if e.pinned {
                true
            } else {
                unpinned_count += 1;
                unpinned_count <= self.max
            }
        });
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
