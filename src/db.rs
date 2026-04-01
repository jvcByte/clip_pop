// SPDX-License-Identifier: MIT

//! SQLite-backed clipboard history store.
//!
//! Schema:
//!   entries(id, mime_type, content BLOB, content_hash TEXT, pinned BOOL, created_at INTEGER)
//!
//! Images are stored as raw bytes (PNG-encoded) directly in the DB — no separate image files.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone};
use rustc_hash::FxHasher;
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
use std::hash::{Hash, Hasher};
use tracing::{error, warn};

use crate::fl;

// ── Entry ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ClipEntry {
    pub id: i64,
    pub mime_type: String,
    /// Raw bytes — UTF-8 text for text/*, PNG bytes for image/*.
    pub content: Vec<u8>,
    pub content_hash: String,
    pub pinned: bool,
    pub created_at: DateTime<Local>,
}

impl ClipEntry {
    /// Single-line preview for display in the list.
    pub fn preview(&self, max_chars: usize) -> String {
        if self.mime_type.starts_with("text/") {
            let text = String::from_utf8_lossy(&self.content);
            let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if collapsed.chars().count() > max_chars {
                let t: String = collapsed.chars().take(max_chars).collect();
                format!("{t}…")
            } else {
                collapsed
            }
        } else if self.mime_type.starts_with("image/") {
            fl!("entry-image")
        } else {
            format!("[{}]", self.mime_type)
        }
    }

    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }

    pub fn is_text(&self) -> bool {
        self.mime_type.starts_with("text/")
    }

    pub fn age_secs(&self) -> i64 {
        (Local::now() - self.created_at).num_seconds()
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

// ── Hash ──────────────────────────────────────────────────────────────────────

pub fn content_hash(data: &[u8]) -> String {
    let mut h = FxHasher::default();
    data.hash(&mut h);
    format!("{:016x}", h.finish())
}

// ── Db ────────────────────────────────────────────────────────────────────────

pub struct Db {
    pool: SqlitePool,
    /// In-memory cache of current entries (newest first, pinned first).
    entries: Vec<ClipEntry>,
    max: usize,
}

impl Db {
    /// Open (or create) the SQLite database at `path`.
    pub async fn open(path: &Path, max: usize) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .context("failed to create db directory")?;
        }

        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(opts).await
            .context("failed to open sqlite db")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS entries (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                mime_type    TEXT    NOT NULL,
                content      BLOB    NOT NULL,
                content_hash TEXT    NOT NULL,
                pinned       INTEGER NOT NULL DEFAULT 0,
                created_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_hash ON entries(content_hash);
            CREATE INDEX IF NOT EXISTS idx_pinned ON entries(pinned, created_at DESC);"
        )
        .execute(&pool)
        .await
        .context("failed to create schema")?;

        let mut db = Self { pool, entries: Vec::new(), max };
        db.reload().await?;
        Ok(db)
    }

    /// Reload the in-memory cache from the database.
    pub async fn reload(&mut self) -> Result<()> {
        let rows = sqlx::query(
            "SELECT id, mime_type, content, content_hash, pinned, created_at
             FROM entries
             ORDER BY pinned DESC, created_at DESC
             LIMIT ?"
        )
        .bind(self.max as i64 + 500) // load a bit more to account for pinned
        .fetch_all(&self.pool)
        .await
        .context("failed to reload entries")?;

        self.entries = rows.iter().map(|r| {
            let ts: i64 = r.get("created_at");
            ClipEntry {
                id: r.get("id"),
                mime_type: r.get("mime_type"),
                content: r.get("content"),
                content_hash: r.get("content_hash"),
                pinned: r.get::<i64, _>("pinned") != 0,
                created_at: Local.timestamp_millis_opt(ts)
                    .single()
                    .unwrap_or_else(Local::now),
            }
        }).collect();

        Ok(())
    }

    pub fn entries(&self) -> &[ClipEntry] {
        &self.entries
    }

    pub fn get(&self, index: usize) -> Option<&ClipEntry> {
        self.entries.get(index)
    }

    #[allow(dead_code)]
    pub fn get_by_id(&self, id: i64) -> Option<&ClipEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Insert a new entry, deduplicating by content hash.
    /// Returns the index of the (new or existing) entry.
    pub async fn insert(&mut self, mime_type: &str, content: Vec<u8>) -> Result<usize> {
        let hash = content_hash(&content);

        // Check for existing entry with same hash
        if let Some(idx) = self.entries.iter().position(|e| e.content_hash == hash) {
            // Already exists — promote it
            self.promote(idx).await?;
            return Ok(0); // after promote it's at index 0 (or after pinned)
        }

        let now = Local::now().timestamp_millis();

        let id = sqlx::query(
            "INSERT INTO entries (mime_type, content, content_hash, pinned, created_at)
             VALUES (?, ?, ?, 0, ?)"
        )
        .bind(mime_type)
        .bind(&content)
        .bind(&hash)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("failed to insert entry")?
        .last_insert_rowid();

        // Trim unpinned entries beyond max
        self.trim_unpinned().await?;

        self.reload().await?;

        let idx = self.entries.iter().position(|e| e.id == id).unwrap_or(0);
        Ok(idx)
    }

    /// Move entry at `index` to the top of the unpinned section.
    pub async fn promote(&mut self, index: usize) -> Result<()> {
        let Some(entry) = self.entries.get(index) else {
            warn!("promote: index {index} out of bounds");
            return Ok(());
        };
        if entry.pinned {
            return Ok(());
        }
        let now = Local::now().timestamp_millis();
        sqlx::query("UPDATE entries SET created_at = ? WHERE id = ?")
            .bind(now)
            .bind(entry.id)
            .execute(&self.pool)
            .await
            .context("failed to promote entry")?;
        self.reload().await
    }

    /// Toggle pin state on entry at `index`.
    pub async fn toggle_pin(&mut self, index: usize) -> Result<()> {
        let Some(entry) = self.entries.get(index) else {
            return Ok(());
        };
        let new_pinned = if entry.pinned { 0i64 } else { 1i64 };
        sqlx::query("UPDATE entries SET pinned = ? WHERE id = ?")
            .bind(new_pinned)
            .bind(entry.id)
            .execute(&self.pool)
            .await
            .context("failed to toggle pin")?;
        self.reload().await
    }

    /// Delete entry at `index`.
    pub async fn remove(&mut self, index: usize) -> Result<()> {
        let Some(entry) = self.entries.get(index) else {
            return Ok(());
        };
        sqlx::query("DELETE FROM entries WHERE id = ?")
            .bind(entry.id)
            .execute(&self.pool)
            .await
            .context("failed to delete entry")?;
        self.entries.remove(index);
        Ok(())
    }

    /// Delete all unpinned entries.
    pub async fn clear_unpinned(&mut self) -> Result<()> {
        sqlx::query("DELETE FROM entries WHERE pinned = 0")
            .execute(&self.pool)
            .await
            .context("failed to clear unpinned")?;
        self.entries.retain(|e| e.pinned);
        Ok(())
    }

    pub fn set_max(&mut self, max: usize) {
        self.max = max;
    }

    async fn trim_unpinned(&mut self) -> Result<()> {
        sqlx::query(
            "DELETE FROM entries WHERE pinned = 0 AND id NOT IN (
                SELECT id FROM entries WHERE pinned = 0
                ORDER BY created_at DESC LIMIT ?
            )"
        )
        .bind(self.max as i64)
        .execute(&self.pool)
        .await
        .context("failed to trim unpinned")?;
        Ok(())
    }
}

/// Search entries using nucleo fuzzy matcher.
pub fn fuzzy_search<'a>(entries: &'a [ClipEntry], query: &str) -> Vec<(usize, &'a ClipEntry)> {
    if query.is_empty() {
        return entries.iter().enumerate().collect();
    }

    let mut matcher = nucleo::Matcher::new(nucleo::Config::DEFAULT);
    let needle = nucleo::pattern::Pattern::parse(
        query,
        nucleo::pattern::CaseMatching::Ignore,
        nucleo::pattern::Normalization::Smart,
    );

    entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let haystack = if entry.is_text() {
                String::from_utf8_lossy(&entry.content).into_owned()
            } else {
                entry.preview(500)
            };
            let mut buf = Vec::new();            let haystack_utf32 = nucleo::Utf32Str::new(&haystack, &mut buf);
            needle
                .score(haystack_utf32, &mut matcher)
                .map(|_score| (i, entry))
        })
        .collect()
}
