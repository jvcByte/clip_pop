# Clip Pop

A clipboard manager for the [COSMIC™](https://system76.com/cosmic) desktop, built with [libcosmic](https://github.com/pop-os/libcosmic) and Rust.

## Features

- Text and image clipboard history
- Persistent history across sessions (`~/.local/share/clip_pop/`)
- Pin items to keep them at the top permanently
- Private mode — pause recording without clearing history
- Search and filter history instantly
- Active item indicator shows what is currently in the clipboard
- Confirm dialog before clearing history (pinned items are always kept)
- Configurable history size and poll interval via COSMIC Settings

## Requirements

- Pop!_OS with COSMIC desktop, or any Linux distribution running COSMIC
- Wayland compositor with `zwlr_data_control` or `ext-data-control` protocol support

## Building

Install dependencies:

```sh
sudo apt install libxkbcommon-dev libwayland-dev libvulkan-dev libdbus-1-dev libssl-dev pkg-config
```

Install the Rust toolchain via [rustup](https://rustup.rs) and [just](https://github.com/casey/just):

```sh
cargo install just
```

Build and run:

```sh
just run
```

## Installation

```sh
just build-release
sudo just install
```

This installs the binary to `/usr/bin/clip_pop`, the `.desktop` entry, appstream metadata, and the app icon.

To install to a custom prefix:

```sh
just rootdir=~/.local prefix='' install
```

## Packaging

Vendor dependencies for offline/reproducible builds:

```sh
just vendor
just build-vendored
just rootdir=debian/clip_pop prefix=/usr install
```

## Configuration

Clip Pop uses `cosmic-config` for persistent settings. The following fields are configurable:

| Field | Default | Description |
|---|---|---|
| `max_history` | 50 | Maximum unpinned entries to retain (10–500) |
| `poll_interval_ms` | 500 | Clipboard poll interval in ms (100–5000) |
| `preview_chars` | 100 | Characters shown in list preview (20–500) |
| `move_to_top_on_select` | true | Move selected item to top of history |
| `private_mode` | false | Pause clipboard recording |

## Data

| Path | Contents |
|---|---|
| `~/.local/share/clip_pop/history.json` | Clipboard history (text + image metadata) |
| `~/.local/share/clip_pop/images/` | Saved clipboard images as PNG |

## Translations

Translations use [Fluent](https://projectfluent.org/). To add a new language, copy `i18n/en/` to `i18n/<language-code>/`, rename the `.ftl` file, and translate each message. The language code should be a valid [ISO 639-1 code](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes).

## License

MIT — see [LICENSE](./LICENSE)
