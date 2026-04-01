# Clip Pop — Roadmap

---

## Changelog

See [CHANGELOG.md](./CHANGELOG.md) for the full version history.

---

## Comparison: cosmic-utils/clipboard-manager

[cosmic-utils/clipboard-manager](https://github.com/cosmic-utils/clipboard-manager) is the reference
COSMIC clipboard applet. Updated comparison after v0.3.0:

| | Clip Pop v0.3.0 | cosmic-utils/clipboard-manager |
|---|---|---|
| Type | Standalone window | COSMIC panel applet |
| Clipboard watching | Event-driven `zwlr_data_control` (blocking_dispatch) | Event-driven `zwlr_data_control` via `cctk` |
| Storage | SQLite via `sqlx` | SQLite via `sqlx` |
| Search | `nucleo` fuzzy search | `nucleo` fuzzy search |
| MIME support | text/*, image/* | Full MIME negotiation |
| Entry expiry | Configurable (days) | Configurable (days) |
| Pagination | None | Configurable entries per page |
| Tests | 7 DB unit tests | SQLite layer has unit tests |
| Pin items | Yes | No |
| Active item indicator | Yes | No |
| Private mode UI toggle | Yes | Config only |
| Confirm before clear | Yes | No |

The primary remaining technical difference is MIME breadth — cosmic-utils negotiates any MIME type the compositor offers (HTML, RTF, file lists, custom types). Clip Pop currently handles `text/*` and `image/*`.

---

## Blockers for v1.0.0

---

### [BUG] cosmic-text patch requires a local sibling directory

**File:** `Cargo.toml`

```toml
[patch.'https://github.com/pop-os/cosmic-text']
cosmic-text = { path = "../cosmic-text" }
```

Anyone cloning the repo cannot build without manually cloning `cosmic-text` at `../cosmic-text` and checking out commit `d5a972a`. Add a `setup.sh` script to automate this until libcosmic pins its own version.

---

### [MISSING] No settings UI

All config fields are only changeable via a third-party `cosmic-config` editor.

**Fix:** Add a Settings context drawer page with controls for each `Config` field.

---

### [MISSING] No keyboard shortcut to open the window

**Fix:** Register a global keybinding via COSMIC's keybinding API, or document how to set one in COSMIC Settings → Keyboard → Shortcuts.

---

## v1.0.0 — Remaining work

---

### [IMPROVEMENT] Full MIME type support

**Current:** Only `text/*` and `image/*` are captured.

**Fix:** Negotiate any MIME type the compositor offers. Store raw bytes with the MIME type. Display appropriate previews per type (HTML rendered, file list as paths, etc.).

---

### [IMPROVEMENT] Pagination

For large histories, paginate the list instead of rendering all entries at once.

---

### [IMPROVEMENT] Settings UI

In-app settings panel for all `Config` fields — max history, preview length, entry lifetime, private mode, preferred MIME types.

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
| 0.1.0 ✓ | Initial release |
| 0.2.0 ✓ | SQLite, wl-clipboard-rs, nucleo, FxHash, private_mode persist |
| 0.3.0 ✓ | Event-driven watcher, entry expiry, DB tests |
| 0.3.1 ✓ | Fix text overflow in history rows |
| 1.0.0 | Settings UI, keyboard shortcut, full MIME, pagination |
