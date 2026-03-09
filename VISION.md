# Amadeuz — Vision & Architecture

## What we're building

An open-source, self-hosted alternative to iCloud, starting with a **Notes app**.
The end goal is a full personal-cloud suite: notes, files, photos, contacts, calendar — all
owned by the user, synced across their devices, running on their own infrastructure.

We are starting with the smallest useful slice: a single synced note that works on every
major platform.

---

## Core principles

### 1. Native, always
No browser engine wrappers. No Electron, no Tauri, no WebView containers.
Every client is written in the idiomatic language and UI framework for its platform.
Users should not be able to tell this isn't a platform-first app.

| Platform | Language / Framework |
|----------|---------------------|
| macOS    | Swift + SwiftUI     |
| iOS      | Swift + SwiftUI     |
| Windows  | C# + WinUI 3        |
| Linux    | C++ + GTK4          |
| Server   | Go                  |

Multiple platform-specific codebases are acceptable and expected.
Shared code is good when it doesn't compromise native feel; it is never worth forcing.

### 2. Offline-first
Every client loads from local disk immediately on startup and works fully without a server.
The server is a sync accelerator, not a dependency for basic use.

### 3. Minimal protocol surface
The wire protocol between client and server is intentionally small and stable.
New features should extend it without breaking existing clients.

### 4. Self-hosted by default
The server runs wherever the user wants: a Raspberry Pi, a home NAS, a VPS.
No vendor lock-in. No accounts unless the user explicitly enables them.

---

## Architecture

```
┌─────────────┐     WebSocket      ┌──────────────────┐
│  macOS app  │ ◄────────────────► │                  │
├─────────────┤                    │   Go sync server  │
│ Windows app │ ◄────────────────► │   :8080/ws        │
├─────────────┤                    │                   │
│  Linux app  │ ◄────────────────► │  note.json        │
├─────────────┤                    │  (disk persistence│
│   iOS app   │ ◄────────────────► │   on the server)  │
└─────────────┘                    └──────────────────┘
       │
  local disk
  (per device)
```

**The server** is a relay + last-known-state store.
It does not own the truth — each client does for itself, offline.

**Each client** owns a local copy of the data.
On connect, it compares timestamps with the server and resolves conflicts.

---

## Wire protocol

All messages are JSON over WebSocket. Field `updated_at` is Unix milliseconds.

```json
// Server → client on connect
{ "type": "init",   "content": "...", "updated_at": 1234567890123 }

// Client → server on user edit
{ "type": "update", "content": "...", "updated_at": 1234567890123 }

// Server → all other clients after accepting an update
{ "type": "update", "content": "...", "updated_at": 1234567890123 }
```

The server only accepts and rebroadcasts an update if its `updated_at` is **strictly greater**
than the currently stored value (last-write-wins).

---

## Local storage format

Every client stores the note identically so the format is readable across platforms:

```json
{ "content": "...", "updatedAt": 1234567890123 }
```

Note: `updatedAt` (camelCase) in local files vs `updated_at` (snake_case) in wire messages.
This matches the original macOS design and all clients follow it.

Storage paths:

| Platform | Path |
|----------|------|
| macOS    | `~/Library/Application Support/amadeuz/note.json` |
| Windows  | `%APPDATA%\amadeuz\note.json` |
| Linux    | `~/.local/share/amadeuz/note.json` |
| Server   | `server/note.json` (next to the binary) |

---

## Sync logic (identical on every client)

This logic lives in `NoteViewModel` (or equivalent) on every platform:

```
on connect → receive "init" from server
  if server.updatedAt > local.updatedAt → accept server content, save locally
  if local.updatedAt > server.updatedAt → push local content to server
  if equal                              → already in sync, do nothing

on user keystroke
  wait 500 ms (debounce)
  if content == last content received from server → skip (echo guard)
  else → save to disk, send "update" to server

on server "update" received
  apply same timestamp comparison as "init"

on disconnect
  wait 3 seconds, reconnect
  continue working offline in the meantime
```

---

## Repository layout

```
amadeuz/
├── VISION.md            ← this file
├── PROGRESS.md          ← per-platform feature status
├── CLAUDE.md            ← project brief for AI sessions
├── .gitignore
├── server/              ← Go sync server
│   ├── go.mod
│   ├── go.sum
│   └── main.go
└── notes/
    ├── mac/             ← macOS (Swift + SwiftUI)
    │   ├── Package.swift
    │   └── Sources/Notes/
    │       ├── NoteApp.swift
    │       ├── ContentView.swift
    │       ├── NoteViewModel.swift
    │       ├── LocalStore.swift
    │       └── SyncService.swift
    ├── windows/         ← Windows (C# + WinUI 3)
    │   └── Amadeuz/
    │       ├── Amadeuz.csproj
    │       ├── MainWindow.xaml / .cs
    │       ├── NoteViewModel.cs
    │       ├── LocalStore.cs
    │       └── SyncService.cs
    ├── linux/           ← Linux (C++ + GTK4)
    │   ├── CMakeLists.txt
    │   └── src/
    │       ├── main.cpp
    │       ├── main_window.h / .cpp
    │       ├── note_view_model.h / .cpp
    │       ├── local_store.h / .cpp
    │       └── sync_service.h / .cpp
    └── ios/             ← iOS — not started
```

---

## Roadmap

### Phase 1 — Single note POC (current)
- [x] Go server
- [x] macOS client
- [x] Windows client
- [x] Linux client
- [ ] iOS client
- [ ] End-to-end test: all platforms syncing over LAN simultaneously

### Phase 2 — Multiple notes
Each note has its own ID. The server stores a map of notes.
The wire protocol adds a `note_id` field. Local storage becomes a directory of JSON files.
UI gains a sidebar or note list.

### Phase 3 — User accounts
Basic auth so the server can serve multiple users.
Credentials stored securely per platform (Keychain on Apple, Credential Manager on Windows,
libsecret on Linux).

### Phase 4 — Conflict resolution
Replace last-write-wins with operational transforms or CRDTs for concurrent edits.

### Phase 5 — Encryption
End-to-end encryption. The server stores and relays ciphertext only.
Keys never leave the client.

### Phase 6 — Files, photos, contacts, calendar
Expand the sync protocol to handle binary blobs, structured records, and calendar events.
The server becomes a full personal-cloud backend.

### Phase 7 — Production deployment
HTTPS/WSS, dynamic DNS or a relay service, proper packaging (`.app`, `.msix`, `.deb`/`.rpm`/flatpak).

---

## Decisions log

| Decision | Rationale |
|----------|-----------|
| One server, many native clients | Native feel on every platform is non-negotiable. Shared UI (web) would compromise it. |
| Go for the server | Excellent concurrency, single static binary, trivial to cross-compile and deploy. |
| gorilla/websocket | De-facto standard WebSocket library for Go; well-maintained. |
| Last-write-wins for POC | Simplest correct merge strategy. Good enough for single-user, single-note. Replace in Phase 4. |
| Unix milliseconds for timestamps | Sufficient resolution; avoids second-level collisions on fast typists; integer comparison. |
| camelCase local / snake_case wire | Local format matches Swift/JSON conventions. Wire format matches Go/server conventions. Decided early and kept consistent across all clients. |
| libsoup-3 for Linux WebSocket | GNOME-native, integrates with GLib main loop, no extra event loop needed alongside GTK4. |
| json-glib for Linux JSON | Same GNOME stack as GTK4 + libsoup; no extra dependency. |
