# Clip Pop — Roadmap

## Current version: 0.1.0

---

## Comparison: cosmic-utils/clipboard-manager

[cosmic-utils/clipboard-manager](https://github.com/cosmic-utils/clipboard-manager) is the reference
COSMIC clipboard applet. Key differences inform what Clip Pop should improve.

| | Clip Pop | cosmic-utils/clipboard-manager |
|---|---|---|
| Type | Standalone window | COSMIC panel applet |
| Clipboard watching | `arboard` polling (500ms) | Event-driven `zwlr_data_control` via `cctk` |
| Storage | `serde_json` flat file | SQLite via `sqlx` |
| Search | Substring `contains()` | `nucleo` fuzzy search |
| MIME support | Text + PNG image only | Full MIME negotiation (HTML, RTF, files, etc.) |
| Entry expiry | None | Configurable lifetime (days) |
| Pagination | None | Configurable entries per page |
| Tests | None | SQLite layer has unit tests |
| Pin items | Yes | No |
| Active item indicator | Yes | No |
| Private mode UI toggle | Yes | Config only |
| Confirm before clear | Yes | No |

**Biggest technical gap:** their clipboard watcher is event-driven using the
`zwlr_data_control` Wayland protocol directly — no polling, no missed events,
full MIME type support. Clip Pop's arboard polling is functional but less
efficient and limited to text and PNG.

---

## Blockers for v1.0.0

These must be resolved before a stable release.

---

### [BUG] cosmic-text patch requires a local sibling directory

**File:** `Cargo.toml`

```toml
[patch.'https://github.com/pop-os/cosmic-text']
cosmic-text = { path = "../cosmic-text" }
```

Anyone cloning the repo cannot build without manually cloning `cosmic-text` at
`../cosmic-text` and checking out commit `d5a972a`. This is a build-time hard
dependency that is invisible to contributors and CI.

**Fix:** Once libcosmic stabilises its `cosmic-text` dependency (or pins it to a
specific rev in its own `Cargo.toml`), remove the patch entirely. Until then,
document the manual step clearly in `README.md` and add a `setup.sh` script
that does the clone automatically.

---

### [BUG] private_mode resets on every restart

**File:** `src/app.rs` — `Message::TogglePrivateMode`

```rust
Message::TogglePrivateMode => {
    self.config.private_mode = !self.config.private_mode;
}
```

The flag is toggled in memory but never written back to `cosmic-config`, so it
is lost on restart.

**Fix:** Persist the change via `cosmic_config::Config`:

```rust
Message::TogglePrivateMode => {
    self.config.private_mode = !self.config.private_mode;
    if let Ok(ctx) = cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
        let _ = self.config.write_entry(&ctx);
    }
}
```

---

### [BUG] active_index for images is approximate

**File:** `src/app.rs` — `Message::ClipboardChanged` image branch

```rust
self.active_index = Some(
    self.history.entries().iter()
        .position(|e| e.kind.is_image())
        .unwrap_or(0),
);
```

This finds the *first* image in history, not the one just added. If there are
multiple images, the indicator dot will point to the wrong entry.

**Fix:** `push_image` should return the index of the inserted entry alongside
the path, so the caller can set `active_index` precisely.

---

### [BUG] DATA_DIR_FALLBACK writes to current working directory

**File:** `src/config.rs`

```rust
pub const DATA_DIR_FALLBACK: &str = ".";
```

If `dirs::data_local_dir()` returns `None`, history is written to wherever the
binary was launched from.

**Fix:**

```rust
dirs::data_local_dir()
    .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
    .join(DATA_DIR_NAME)
    .join(HISTORY_FILE_NAME)
```

---

### [BUG] DefaultHasher is not stable across Rust versions

**File:** `src/app.rs` — `content_hash()`, `src/history.rs` — `simple_hash()`

`std::collections::hash_map::DefaultHasher` output can change between Rust
releases. A hash collision would silently drop a clipboard event or fail to
deduplicate an image.

**Fix:** Use a stable deterministic hash:

```toml
# Cargo.toml
rustc-hash = "2"
```

```rust
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

fn content_hash(data: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    data.hash(&mut h);
    h.finish()
}
```

---

### [BUG] Image hash stored in filename — fragile dedup

**File:** `src/history.rs` — `push_image()`

The content hash is recovered by parsing the filename. If the file is renamed
or the naming scheme changes, dedup silently breaks.

**Fix:** Store the hash directly in `EntryKind::Image`:

```rust
pub enum EntryKind {
    Text { content: String },
    Image { path: PathBuf, width: u32, height: u32, content_hash: u64 },
}
```

Note: this is a breaking change to `history.json` — bump `Config::VERSION` and
handle migration.

---

### [MISSING] No LICENSE file

The repository declares `license = "MIT"` in `Cargo.toml` but there is no
`LICENSE` file. Add a standard MIT `LICENSE` file.

---

## Should-fix for v1.0.0

---

### [FEATURE] No settings UI

All config fields are only changeable via a third-party `cosmic-config` editor.

**Fix:** Add a Settings context drawer page with controls for each `Config`
field, writing changes back via `cosmic_config::Config`.

---

### [FEATURE] No keyboard shortcut to open the window

**Fix:** Register a global keybinding via COSMIC's keybinding API, or document
how to set one in COSMIC Settings → Keyboard → Shortcuts.

---

### [FEATURE] No way to copy without moving to top

**Fix:** Add a secondary action that copies without promoting the item.

---

## v0.3.0 — Technical improvements (informed by cosmic-utils comparison)

---

### [IMPROVEMENT] Replace arboard polling with event-driven zwlr_data_control

**Current:** `arboard` polls the clipboard every 500ms.

**Problem:** Polling misses rapid copies, wastes CPU, and is limited to text
and PNG. arboard does not support full MIME negotiation.

**Fix:** Implement a dedicated clipboard watcher using `cctk`
(`cosmic::cctk`) and the `zwlr_data_control_v1` Wayland protocol directly,
the same approach used by `cosmic-utils/clipboard-manager`. This gives:

- Event-driven — zero polling, instant capture
- Full MIME type support (HTML, RTF, file lists, custom types)
- Correct handling of multiple Wayland seats
- No dependency on arboard at all

Reference: [`src/clipboard_watcher.rs`](https://github.com/cosmic-utils/clipboard-manager/blob/master/src/clipboard_watcher.rs)
in cosmic-utils/clipboard-manager.

---

### [IMPROVEMENT] Replace serde_json flat file with SQLite

**Current:** History is a single JSON array written on every change.

**Problem:** Slow on large histories, no querying, no entry expiry, no
pagination, no atomic writes.

**Fix:** Use `sqlx` with SQLite (same as cosmic-utils/clipboard-manager):

- Atomic writes
- Entry lifetime expiry (e.g. auto-delete entries older than 30 days)
- Pagination for large histories
- Proper indexing for fast search

---

### [IMPROVEMENT] Replace substring search with fuzzy search

**Current:** `e.kind.dedup_key().to_lowercase().contains(&query)`

**Fix:** Use `nucleo` (same as cosmic-utils/clipboard-manager) for fuzzy
matching. Significantly better UX when searching long histories.

```toml
nucleo = "0.5"
```

---

### [IMPROVEMENT] Full MIME type support

**Current:** Only text (`get_text()`) and PNG images (`get_image()`) are
captured.

**Fix:** With the `zwlr_data_control` watcher, negotiate MIME types and store
the preferred type per entry. Support at minimum:
- `text/plain`
- `text/html`
- `image/png`, `image/jpeg`, `image/webp`
- `text/uri-list` (file paths)

---

## Nice to have (post v1.0.0)

- Image preview on hover / expand
- Regex search mode
- Auto-clear history on interval
- Exclude specific apps from being recorded
- Keyboard navigation within the list (arrow keys, Enter to select)
- Primary selection support
- QR code generation for selected text (libcosmic has `qr_code` feature)
- Export / import history

---

## Version plan

| Version | Goal |
|---------|------|
| 0.1.0 | Initial release — core functionality working |
| 0.2.0 | Fix all v1.0.0 blockers, add LICENSE, settings UI |
| 0.3.0 | Event-driven watcher, SQLite storage, fuzzy search, full MIME |
| 1.0.0 | Stable, all blockers resolved, feature-complete |
