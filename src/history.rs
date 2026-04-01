// SPDX-License-Identifier: MIT

//! Clipboard history entry types and persistence.

use crate::fl;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, warn};

// ── Entry kind ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EntryKind {
    Text { content: String },
    Image { path: PathBuf, width: u32, height: u32 },
}

impl EntryKind {
    /// Single-line preview string for display.
    pub fn preview(&self, max_chars: usize) -> String {
        match self {
            EntryKind::Text { content } => {
                let collapsed: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
                if collapsed.chars().count() > max_chars {
                    let t: String = collapsed.chars().take(max_chars).collect();
                    format!("{t}…")
                } else {
                    collapsed
                }
            }
            EntryKind::Image { width, height, .. } => {
                format!("🖼  {width}×{height} image")
            }
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, EntryKind::Image { .. })
    }

    /// Stable dedup key — for text it's the content, for images the file path.
    pub fn dedup_key(&self) -> String {
        match self {
            EntryKind::Text { content } => content.clone(),
            EntryKind::Image { path, .. } => path.to_string_lossy().into_owned(),
        }
    }
}

// ── ClipEntry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipEntry {
    pub kind: EntryKind,
    pub timestamp: DateTime<Local>,
    pub pinned: bool,
}

impl ClipEntry {
    pub fn text(content: String) -> Self {
        Self { kind: EntryKind::Text { content }, timestamp: Local::now(), pinned: false }
    }

    pub fn image(path: PathBuf, width: u32, height: u32) -> Self {
        Self {
            kind: EntryKind::Image { path, width, height },
            timestamp: Local::now(),
            pinned: false,
        }
    }

    pub fn preview(&self, max_chars: usize) -> String {
        self.kind.preview(max_chars)
    }

    pub fn age_secs(&self) -> i64 {
        (Local::now() - self.timestamp).num_seconds()
    }

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

// ── HistoryStore ──────────────────────────────────────────────────────────────

pub struct HistoryStore {
    entries: Vec<ClipEntry>,
    path: PathBuf,
    images_dir: PathBuf,
    max: usize,
}

impl HistoryStore {
    pub fn load(path: PathBuf, max: usize) -> Self {
        let images_dir = path.parent().unwrap_or(Path::new(".")).join("images");
        let entries = fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<Vec<ClipEntry>>(&data).ok())
            .unwrap_or_default();
        Self { entries, path, images_dir, max }
    }

    pub fn entries(&self) -> &[ClipEntry] {
        &self.entries
    }

    pub fn images_dir(&self) -> &Path {
        &self.images_dir
    }

    /// Add a text entry. Returns `true` if modified.
    pub fn push_text(&mut self, content: String) -> bool {
        if content.trim().is_empty() {
            return false;
        }
        let key = EntryKind::Text { content: content.clone() }.dedup_key();
        self.entries.retain(|e| e.pinned || e.kind.dedup_key() != key);
        let insert_at = self.first_unpinned();
        self.entries.insert(insert_at, ClipEntry::text(content));
        self.trim_unpinned();
        self.persist();
        true
    }

    /// Save raw RGBA image data as PNG and add an image entry.
    /// Returns the saved path on success.
    pub fn push_image(&mut self, rgba: &[u8], width: u32, height: u32) -> Option<PathBuf> {
        if let Err(e) = fs::create_dir_all(&self.images_dir) {
            error!("failed to create images dir: {e}");
            return None;
        }

        let filename = format!("{}.png", Local::now().format("%Y%m%d_%H%M%S_%f"));
        let img_path = self.images_dir.join(&filename);

        // Encode RGBA → PNG
        let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())?;
        if let Err(e) = img.save(&img_path) {
            error!("failed to save clipboard image: {e}");
            return None;
        }

        let insert_at = self.first_unpinned();
        self.entries.insert(insert_at, ClipEntry::image(img_path.clone(), width, height));
        self.trim_unpinned();
        self.persist();
        Some(img_path)
    }

    pub fn promote(&mut self, index: usize) -> Option<&ClipEntry> {
        if index >= self.entries.len() {
            warn!("promote: index {index} out of bounds");
            return None;
        }
        if self.entries[index].pinned {
            return self.entries.get(index);
        }
        let entry = self.entries.remove(index);
        let insert_at = self.first_unpinned();
        self.entries.insert(insert_at, entry);
        self.persist();
        self.entries.get(insert_at)
    }

    pub fn toggle_pin(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        let mut entry = self.entries.remove(index);
        entry.pinned = !entry.pinned;
        let insert_at = if entry.pinned {
            self.entries.iter().rposition(|e| e.pinned).map_or(0, |p| p + 1)
        } else {
            self.first_unpinned()
        };
        self.entries.insert(insert_at, entry);
        self.persist();
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        // Delete image file if applicable
        if let EntryKind::Image { path, .. } = &self.entries[index].kind {
            if let Err(e) = fs::remove_file(path) {
                warn!("failed to delete image file: {e}");
            }
        }
        self.entries.remove(index);
        self.persist();
    }

    pub fn clear_unpinned(&mut self) {
        // Delete image files for unpinned image entries
        for entry in self.entries.iter().filter(|e| !e.pinned) {
            if let EntryKind::Image { path, .. } = &entry.kind {
                let _ = fs::remove_file(path);
            }
        }
        self.entries.retain(|e| e.pinned);
        self.persist();
    }

    pub fn set_max(&mut self, max: usize) {
        self.max = max;
        self.trim_unpinned();
        self.persist();
    }

    fn first_unpinned(&self) -> usize {
        self.entries.iter().position(|e| !e.pinned).unwrap_or(self.entries.len())
    }

    fn trim_unpinned(&mut self) {
        let mut count = 0usize;
        let mut to_delete: Vec<PathBuf> = Vec::new();
        self.entries.retain(|e| {
            if e.pinned {
                true
            } else {
                count += 1;
                if count > self.max {
                    if let EntryKind::Image { path, .. } = &e.kind {
                        to_delete.push(path.clone());
                    }
                    false
                } else {
                    true
                }
            }
        });
        for path in to_delete {
            let _ = fs::remove_file(path);
        }
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
