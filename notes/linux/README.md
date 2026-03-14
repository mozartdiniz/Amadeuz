# Amadeuz Notes — Linux

Native GNOME notes app built with Rust, GTK4, and Libadwaita.
Part of the [Amadeuz](../../README.md) self-hosted notes project.

---

## Features

- **Offline mode** — works with no server; notes saved locally. Choose on first launch.
- **Sync** — connect to your own Amadeuz server for live sync across devices.
- **3-column layout** — folders | note list | editor, adaptive (collapses on small screens).
- **Folders** — create inline (no modal), rename via context menu, delete with cascade.
- **Trash** — move notes to trash, restore, or delete permanently.
- **Search** — toggle search bar in the editor header; filters across all notes.
- **Live Markdown** — headings, bold, italic, code, bullet lists styled in real time.
- **Smart Enter** — continues list items automatically; empty marker exits the list.
- **Inline images** — drag image files from your file manager, or paste screenshots (Ctrl+V).
- **Auth** — register, login, and recover account (recovery codes; no email required).
- **JWT in keyring** — credentials stored in GNOME Keyring via libsecret.

---

## Stack

| Layer | Choice |
|-------|--------|
| Language | Rust (gtk4-rs 0.10, glib 0.21) |
| Toolkit | GTK4 + Libadwaita 1.8 |
| UI | Blueprint 0.18 (`.blp` → compiled `.ui`) |
| Build | Meson + Cargo |
| Config | GSettings (`com.amadeuz.Notes`) |
| Credentials | libsecret / GNOME Keyring |
| Async | Tokio + tokio-tungstenite + reqwest |
| Markdown | pulldown-cmark 0.12 (pure Rust) |
| Local data | `~/.local/share/amadeuz/data.json` |
| Images | `~/.local/share/amadeuz/images/{note_id}/` |

---

## Build & Run

### Dependencies (Fedora)

```bash
sudo dnf install meson rust cargo libadwaita-devel libsecret-devel blueprint-compiler
```

**Pop!_OS / Ubuntu / Debian:**
```bash
sudo apt update && sudo apt install -y \
    build-essential \
    pkg-config \
    meson \
    rustc \
    cargo \
    libadwaita-1-dev \
    libsecret-1-dev \
    libglib2.0-dev \
    libgtk-4-dev \
    libgraphene-1.0-dev \
    libxml2-utils \
    blueprint-compiler

### First-time setup

```bash
cd notes/linux
meson setup build --prefix=$HOME/.local -Dprofile=development
```

### Build and install

```bash
just build
# or manually:
ninja -C build && meson install -C build
glib-compile-schemas ~/.local/share/glib-2.0/schemas/
```

### Run

```bash
just dev
# or:
GSETTINGS_SCHEMA_DIR=~/.local/share/glib-2.0/schemas ~/.local/bin/amadeuz-notes
```

### Other just commands

```
just setup       # first-time meson setup
just build       # build + install
just dev         # build + install + run
just run         # run without rebuilding
just run-debug   # run with RUST_LOG=debug
just build-clean # rm -rf build, full rebuild from scratch
```

---

## Project layout

```
notes/linux/
├── justfile                 ← build shortcuts
├── meson.build              ← project, dependencies, subdir() calls
├── Cargo.toml               ← Rust dependencies
├── data/
│   ├── meson.build          ← Blueprint compile, GResource bundle, gschema install
│   ├── com.amadeuz.Notes.gschema.xml   ← GSettings schema (server-url key)
│   ├── com.amadeuz.Notes.gresource.xml ← GResource manifest
│   └── ui/
│       ├── window.blp       ← main window (welcome / auth / 3-column layout)
│       ├── auth.blp         ← auth view (login / register / recover)
│       └── note_row.blp     ← note list row (title + preview + folder · date)
└── src/
    ├── main.rs              ← entry point: tokio runtime, GResource, GTK app
    ├── config.rs            ← compile-time constants from Meson (APP_ID, DATADIR…)
    ├── app.rs               ← AmzApplication: startup, keyring, offline detection
    ├── manager.rs           ← NotesManager: event loop, auth, CRUD, sync, debounce
    ├── model/
    │   ├── note.rs          ← AmzNote GObject
    │   └── folder.rs        ← AmzFolder GObject
    ├── ui/
    │   ├── window.rs        ← AmzWindow: all UI wiring and callbacks
    │   ├── auth.rs          ← AmzAuthView: login/register/recover composite widget
    │   ├── note_row.rs      ← AmzNoteRow: note list row composite widget
    │   └── md_formatter.rs  ← Markdown TextTag formatting, smart Enter, image embed
    └── backend/
        ├── api_client.rs    ← REST API client (all endpoints)
        ├── local_store.rs   ← load/save data.json
        ├── keyring.rs       ← JWT save/load/delete via libsecret
        └── sync_worker.rs   ← per-note WebSocket, auto-reconnect
```

---

## Server

See [`server/`](../../server/) for the Go sync server.
Default: `http://localhost:8080`. Configurable via the "Server settings" menu item.

The Linux client works fully in **offline mode** with no server running.
