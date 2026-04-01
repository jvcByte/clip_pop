# Clip Pop — Roadmap

## Current version: 0.1.0

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
document the manual step clearly in `README.md` and consider adding a
`setup.sh` script that does the clone automatically.

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

**Fix:** `push_image` should return the index of the inserted entry, or
`HistoryStore` should expose a method that returns the index alongside the path.

---

### [BUG] DATA_DIR_FALLBACK writes to current working directory

**File:** `src/config.rs`

```rust
pub const DATA_DIR_FALLBACK: &str = ".";
```

If `dirs::data_local_dir()` returns `None` (rare but possible in minimal
environments), history is written to wherever the binary was launched from.

**Fix:** Fall back to `$HOME/.local/share/clip_pop` explicitly:

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

`std::collections::hash_map::DefaultHasher` is explicitly documented as
unstable — its output can change between Rust releases. A hash collision would
cause a clipboard event to be silently dropped (missed dedup) or an image to
not be recognised as a duplicate.

**Fix:** Use a stable, deterministic hash. Add `rustc-hash` or inline a simple
`FxHash`/`djb2` implementation:

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

```rust
// Deduplicate by content hash before touching disk
let hash = simple_hash(rgba);
if let Some(existing) = self.entries.iter().find(|e| {
    matches!(&e.kind, EntryKind::Image { path, .. } if {
        path.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split('_').next())
            .and_then(|h| h.parse::<u64>().ok())
            == Some(hash)
    })
}) { ... }
```

The hash is recovered by parsing the filename. If the file is renamed, moved,
or the naming scheme changes, dedup silently breaks.

**Fix:** Store the hash directly in `EntryKind::Image`:

```rust
pub enum EntryKind {
    Text { content: String },
    Image { path: PathBuf, width: u32, height: u32, content_hash: u64 },
}
```

Dedup then becomes a simple field comparison with no string parsing.
Note: this is a breaking change to the `history.json` schema — bump
`Config::VERSION` and add a migration path.

---

### [MISSING] No LICENSE file

The repository declares `license = "MIT"` in `Cargo.toml` but there is no
`LICENSE` file in the repo root. This means the licence is not legally
enforceable and crates.io / packaging tools will warn.

**Fix:** Add a standard MIT `LICENSE` file.

---

## Should-fix for v1.0.0

These are not hard blockers but significantly affect user experience.

---

### [FEATURE] No settings UI

All configuration fields (`max_history`, `poll_interval_ms`, `preview_chars`,
`move_to_top_on_select`) are only changeable via a third-party `cosmic-config`
editor. Users have no in-app way to adjust settings.

**Fix:** Add a Settings context drawer page with sliders/toggles for each
`Config` field, writing changes back via `cosmic_config::Config`.

---

### [FEATURE] No keyboard shortcut to open the window

There is no global hotkey to bring Clip Pop to the foreground. Users must
find it in the app launcher or taskbar.

**Fix:** Register a global keybinding (e.g. `Super+V`) via COSMIC's keybinding
API, or document how to set one in COSMIC Settings → Keyboard → Shortcuts.

---

### [FEATURE] No way to copy without moving to top

`move_to_top_on_select` is configurable but there is no per-action override.
Power users may want to copy an item without disturbing the history order.

**Fix:** Add a secondary action (e.g. right-click or a dedicated copy button)
that copies without promoting.

---

## Nice to have (post v1.0.0)

- Image preview on hover / expand
- Regex search mode
- Auto-clear history on interval (like clipboard-indicator)
- Exclude specific apps from being recorded
- Keyboard navigation within the list (arrow keys, Enter to select)
- Multiple clipboard support (primary selection)

---

## Version plan

| Version | Goal |
|---------|------|
| 0.1.0 | Initial release — core functionality working |
| 0.2.0 | Fix all blockers listed above |
| 0.3.0 | Settings UI, keyboard shortcut |
| 1.0.0 | Stable, all blockers resolved, settings UI complete |
