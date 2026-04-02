# Clip Pop

A clipboard manager for the [COSMIC™](https://system76.com/cosmic) desktop, built with [libcosmic](https://github.com/pop-os/libcosmic) and Rust.

## Features

- Text and image clipboard history — captured instantly via the `zwlr_data_control` Wayland protocol
- `Super+V` keyboard shortcut to open from anywhere
- Persistent history across sessions (SQLite database)
- Fuzzy search powered by [nucleo](https://github.com/helix-editor/nucleo)
- Pin items to keep them at the top permanently
- Private mode — pause recording without clearing history (persisted across restarts)
- Active item indicator shows what is currently in the clipboard
- Confirm dialog before clearing history (pinned items are always kept)
- Auto-expire entries older than a configurable number of days (default 30)
- Configurable history size and preview length

## Requirements

- Pop!_OS with COSMIC desktop, or any Linux distribution running COSMIC
- Wayland compositor with `zwlr_data_control` or `ext-data-control` protocol support (COSMIC, Sway, KDE, Hyprland)

## Building from source

```sh
# 1. Install system dependencies
sudo apt install libxkbcommon-dev libwayland-dev libvulkan-dev libdbus-1-dev libssl-dev pkg-config

# 2. Install Rust toolchain and just
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just

# 3. Build and run
just run
```

## Installation

```sh
just build-release
sudo just install
```

Installs to `/usr/bin/clip_pop`, `/usr/share/applications/`, `/usr/share/icons/`, and `/usr/share/appdata/`.

For a custom prefix:

```sh
just rootdir=~/.local prefix='' install
```

For development (no sudo, installs icon only):

```sh
just dev-install
```

## Running tests

```sh
cargo test --bin clip_pop -- db::tests
```

## Packaging

```sh
just vendor
just build-vendored
just rootdir=debian/clip_pop prefix=/usr install
```

## Configuration

Clip Pop uses `cosmic-config` for persistent settings. Fields are configurable via any `cosmic-config` compatible editor.

| Field | Default | Description |
|---|---|---|
| `max_history` | 50 | Maximum unpinned entries to retain (10–500) |
| `preview_chars` | 100 | Characters shown in list preview (20–500) |
| `move_to_top_on_select` | true | Move selected item to top of history |
| `private_mode` | false | Pause clipboard recording |
| `entry_lifetime_days` | 30 | Auto-delete unpinned entries older than N days (`null` to disable) |
| `preferred_mime_types` | `[]` | Regex patterns for preferred MIME types |

## Data

| Path | Contents |
|---|---|
| `~/.local/share/clip_pop/history.db` | SQLite clipboard history database |

## Architecture

```
src/
  main.rs              # entry point, tracing init, window settings
  app.rs               # AppModel, view, update, subscription
  clipboard.rs         # subscription — tries event-driven watcher, falls back to polling
  clipboard_watcher.rs # zwlr_data_control_v1 Wayland protocol, blocking_dispatch
  config.rs            # Config struct, constants, PRIVATE_MODE atomic
  db.rs                # SQLite store, fuzzy search, unit tests
  i18n.rs              # Fluent localisation
```

## Translations

Translations use [Fluent](https://projectfluent.org/). Copy `i18n/en/` to `i18n/<language-code>/`, rename the `.ftl` file, and translate each message. Language codes follow [ISO 639-1](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes).

## License

MIT — see [LICENSE](./LICENSE) — [Changelog](./CHANGELOG.md)
