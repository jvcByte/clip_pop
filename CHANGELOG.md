# Changelog

All notable changes to Clip Pop are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/).

---

## [0.3.1] — 2026-04-01

### Fixed
- Text entries with no word boundaries (long URLs, code, base64 strings) overflowed horizontally past the pin and delete buttons. Added `WordOrGlyph` wrapping to the text widget.

---

## [0.3.0] — 2026-04-01

### Added
- Event-driven clipboard watcher (`src/clipboard_watcher.rs`) — implements `zwlr_data_control_v1` directly via `wayland-client`. Uses `blocking_dispatch()`, fires instantly on copy with zero polling. Falls back to 100ms polling if the compositor doesn't support the protocol.
- Entry lifetime expiry — `entry_lifetime_days` config field (default 30 days). Unpinned entries older than the limit are deleted on startup.
- 7 DB unit tests covering insert, deduplication, pin, promote, trim, remove, and expiry.

### Dependencies
- Added: `wayland-client`, `wayland-protocols-wlr`, `os_pipe`, `thiserror`
- Added (dev): `tempfile`

---

## [0.2.0] — 2026-04-01

### Added
- Full MIME type support — text and image MIME types negotiated from the compositor.
- Error state banner if the compositor doesn't support the required Wayland protocol.

### Changed
- Clipboard watcher replaced `arboard` polling with `wl-clipboard-rs` using `zwlr_data_control` / `ext-data-control`. No focus requirement. Poll interval 100ms.
- Storage replaced `serde_json` flat file with SQLite via `sqlx`. History stored in `~/.local/share/clip_pop/history.db`. Images stored as raw bytes in the DB — no separate image files.
- Search replaced substring `contains()` with `nucleo` fuzzy matcher.
- Content hashing replaced `DefaultHasher` (unstable across Rust versions) with `FxHasher` from `rustc-hash`.
- Active item now tracked by stable DB row ID instead of array index — survives pin and reorder operations.
- `private_mode` toggle now persists to `cosmic-config` immediately.
- Data directory fallback now uses `$HOME/.local/share` then `/tmp` instead of `.`.

### Added
- `LICENSE` file (MIT).

---

## [0.1.0] — 2026-04-01

### Added
- Initial release.
- Text and image clipboard history via `arboard` polling (500ms).
- Persistent history across sessions (`~/.local/share/clip_pop/history.json`).
- Pin items to keep them at the top permanently.
- Private mode — pause recording without clearing history.
- Active item indicator — dot marks the entry currently in the clipboard.
- Search and filter history.
- Confirm dialog before clearing history (pinned items kept).
- Per-item delete and pin/unpin actions.
- i18n via Fluent with plural-aware relative timestamps.
- XDG packaging — `.desktop`, `.metainfo.xml`, hicolor icon.
