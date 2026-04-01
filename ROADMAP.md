# Clip Pop — Roadmap

---

## Changelog

### v0.2.0 (current)

- **Clipboard watcher** — replaced `arboard` polling with `wl-clipboard-rs` using the `zwlr_data_control` / `ext-data-control` Wayland protocol directly. No focus requirement. Poll interval reduced to 100ms.
- **Storage** — replaced `serde_json` flat file with SQLite via `sqlx`. History stored in `~/.local/share/clip_pop/history.db`. Images stored as raw bytes in the DB — no separate image files.
- **Search** — replaced substring `contains()` with `nucleo` fuzzy matcher (same library used by cosmic-utils/clipboard-manager).
- **Full MIME support** — text and image MIME types negotiated from the compositor. Priority order configurable via `preferred_mime_types` in config.
- **Stable hashing** — replaced `DefaultHasher` (unstable across Rust versions) with `FxHasher` from `rustc-hash`.
- **active_id** — tracks the active clipboard entry by stable DB row ID instead of array index. Survives pin/reorder operations.
- **private_mode persisted** — toggling private mode now writes to `cosmic-config` immediately.
- **DATA_DIR fallback** — falls back to `$HOME/.local/share` then `/tmp` instead of `.`.
- **LICENSE** — MIT license file added.
- **Error state** — shows a banner if the compositor doesn't support the required Wayland protocol.

### v0.1.0

- Initial release — text and image clipboard history, pin items, private mode, search, confirm clear, active item indicator, persistent JSON storage.

---

## Comparison: cosmic-utils/clipboard-manager

[cosmic-utils/clipboard-manager](https://github.com/cosmic-utils/clipboard-manager) is the reference
COSMIC clipboard applet. Updated comparison after v0.2.0:

| | Clip Pop v0.2.0 | cosmic-utils/clipboard-manager |
|---|---|---|
| Type | Standalone window | COSMIC panel applet |
| Clipboard watching | `wl-clipboard-rs` 100ms poll | Event-driven `zwlr_data_control` via `cctk` |
| Storage | SQLite via `sqlx` | SQLite via `sqlx` |
| Search | `nucleo` fuzzy search | `nucleo` fuzzy search |
| MIME support | text/*, image/* | Full MIME negotiation |
| Entry expiry | None | Configurable lifetime (days) |
| Pagination | None | Configurable entries per page |
| Tests | None | SQLite layer has unit tests |
| Pin items | Yes | No |
| Active item indicator | Yes | No |
| Private mode UI toggle | Yes | Config only |
| Confirm before clear | Yes | No |

**Remaining gap:** their watcher is fully event-driven (zero polling) using `cctk` and a custom Wayland dispatch loop. Clip Pop still polls at 100ms. This is the primary remaining technical difference.

---

## Blockers for v1.0.0

---

### [BUG] cosmic-text patch requires a local sibling directory

**File:** `Cargo.toml`

```toml
[patch.'https://github.com/pop-os/cosmic-text']
cosmic-text = { path = "../cosmic-text" }
```

Anyone cloning the repo cannot build without manually cloning `cosmic-text` at
`../cosmic-text` and checking out commit `d5a972a`.

**Fix:** Once libcosmic pins its own `cosmic-text` version, remove the patch. Until then, a `setup.sh` script should automate the clone step.

---

### [MISSING] No settings UI

All config fields are only changeable via a third-party `cosmic-config` editor.

**Fix:** Add a Settings context drawer page with controls for each `Config` field.

---

### [MISSING] No keyboard shortcut to open the window

**Fix:** Register a global keybinding via COSMIC's keybinding API, or document how to set one in COSMIC Settings → Keyboard → Shortcuts.

---

## v0.3.0 — Remaining improvements

---

### [IMPROVEMENT] True event-driven clipboard watcher

**Current:** `wl-clipboard-rs` polled at 100ms.

**Problem:** `wl-clipboard-rs` doesn't expose a blocking "wait for change" API, so we still poll. Rapid copies within 100ms could be missed.

**Fix:** Implement a custom Wayland event loop using `wayland-client` directly (as cosmic-utils does in `clipboard_watcher.rs`), blocking on `dispatch()` until a `Selection` event fires. Zero polling, instant capture.

Reference: [`src/clipboard_watcher.rs`](https://github.com/cosmic-utils/clipboard-manager/blob/master/src/clipboard_watcher.rs)

---

### [IMPROVEMENT] Entry lifetime expiry

Auto-delete entries older than a configurable number of days (e.g. 30).

---

### [IMPROVEMENT] Pagination

For large histories, paginate the list instead of rendering all entries at once.

---

### [IMPROVEMENT] Unit tests for the DB layer

Add tests for `db.rs` — insert, dedup, promote, pin, clear, trim.

---

## Nice to have (post v1.0.0)

- Image preview on hover / expand
- Regex search mode
- Auto-clear history on interval
- Exclude specific apps from being recorded
- Keyboard navigation (arrow keys, Enter to select)
- Primary selection support
- QR code generation for selected text
- Export / import history

---

## Version plan

| Version | Goal |
|---------|------|
| 0.1.0 | Initial release |
| 0.2.0 ✓ | SQLite, wl-clipboard-rs, nucleo, FxHash, private_mode persist |
| 0.3.0 | True event-driven watcher, entry expiry, pagination, DB tests |
| 1.0.0 | Settings UI, keyboard shortcut, all blockers resolved |
