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

### 5. End-to-end encrypted by default
The server is a routing and storage layer, not a trust boundary. Note content is encrypted
on the client before transmission. The server stores and relays ciphertext only.
The server operator cannot read user data even with full database access.

### 6. Modular server — one binary, opt-in features
The server ships as a single binary. Features (notes, files, contacts, reminders) are
enabled or disabled in a config file at startup. This keeps self-hosting simple: one
process, one database file, one config. No orchestration required.

---

## Architecture

```
┌─────────────┐   REST + WebSocket   ┌──────────────────────────────┐
│  macOS app  │ ◄──────────────────► │                              │
├─────────────┤                      │   Go sync server (modular)   │
│ Windows app │ ◄──────────────────► │   :8080                      │
├─────────────┤                      │                              │
│  Linux app  │ ◄──────────────────► │   SQLite (amadeuz.db)        │
├─────────────┤                      │                              │
│   iOS app   │ ◄──────────────────► │   internal packages:         │
└─────────────┘                      │     auth / notes / folders   │
       │                             │     hub / users / config     │
  local disk                         └──────────────────────────────┘
  (per device,
   encrypted)
```

**The server** is a routing and storage layer. It enforces who can access what,
but it cannot read note content — all content arrives as ciphertext.

**Each client** owns a local copy of the data and its own encryption keys.
On connect, it compares timestamps with the server and resolves conflicts.
Keys are stored in the platform's secure credential store (Keychain on Apple,
Credential Manager on Windows, libsecret on Linux).

---

## Wire protocol

All messages are JSON. `updated_at` is Unix milliseconds.

### REST endpoints (authenticated via JWT in Authorization header)

```
POST /auth/register        → create account, returns JWT
POST /auth/login           → returns JWT

GET  /folders              → list user's folders
POST /folders              → create folder
DELETE /folders/{id}       → delete folder

GET  /notes                → list user's notes (metadata, no content)
POST /notes                → create note
GET  /notes/{id}           → fetch note (ciphertext + key envelope for caller)
PATCH /notes/{id}          → update note
DELETE /notes/{id}         → delete note

GET  /notes/{id}/shares    → list collaborators
POST /notes/{id}/share     → share with another user (sends key envelope)
DELETE /notes/{id}/share/{uid} → revoke access

GET  /users/search?email=  → look up user's public key (for sharing)
```

### WebSocket (per note, authenticated via ?token=<jwt>)

```
wss://server/notes/{id}/ws?token=<jwt>
```

```json
// Server → client on connect
{ "type": "init", "note_id": "uuid", "content": "<ciphertext>", "updated_at": 1234567890123 }

// Client → server on user edit
{ "type": "update", "note_id": "uuid", "content": "<ciphertext>", "updated_at": 1234567890123 }

// Server → all other clients in the same note room
{ "type": "update", "note_id": "uuid", "content": "<ciphertext>", "updated_at": 1234567890123 }
```

`content` is base64url-encoded AES-256-GCM ciphertext (nonce prepended).
The server applies the same last-write-wins rule: only accepts and rebroadcasts if
`updated_at` is strictly greater than stored.

---

## Local storage format

Each note is stored as a separate file keyed by note ID. With E2E encryption, `content`
is ciphertext (base64url). The note key is stored in the platform's secure credential
store, not alongside the data file.

```json
{ "content": "<ciphertext or plaintext>", "updatedAt": 1234567890123 }
```

Note: `updatedAt` (camelCase) in local files vs `updated_at` (snake_case) in wire messages.
This matches the original macOS design and all clients follow it.

Storage paths:

| Platform | Notes directory | Credential store |
|----------|-----------------|-----------------|
| macOS    | `~/Library/Application Support/amadeuz/notes/` | Keychain |
| Windows  | `%APPDATA%\amadeuz\notes\` | Windows Credential Manager |
| Linux    | `~/.local/share/amadeuz/notes/` | libsecret / GNOME Keyring |
| Server   | `amadeuz.db` (SQLite, ciphertext only) | — |

---

## Sync logic (identical on every client)

This logic lives in `NoteEditorViewModel` (or equivalent) on every platform.

**Important (E2E encryption):** the echo guard must use timestamp comparison only —
not content equality. Two encryptions of the same plaintext produce different ciphertext
(AES-GCM uses a random nonce). Comparing ciphertext blobs would always treat own-sent
updates as new content and cause an infinite loop.

```
on connect → authenticate with JWT, open WS to /notes/{id}/ws?token=<jwt>
           → receive "init" from server (ciphertext + timestamp)
  decrypt content with note key
  if server.updatedAt > local.updatedAt → accept, save locally
  if local.updatedAt > server.updatedAt → encrypt local, push to server
  if equal                              → already in sync, do nothing

on user keystroke
  wait 500 ms (debounce)
  if current updatedAt == last received updatedAt → skip (echo guard by timestamp)
  else → encrypt content, save ciphertext to disk, send "update" to server

on server "update" received
  decrypt, apply same timestamp comparison as "init"

on disconnect
  wait 3 seconds, reconnect
  continue working offline in the meantime (local decrypted content stays in memory)
```

---

## Repository layout

```
amadeuz/
├── VISION.md            ← this file
├── PROGRESS.md          ← per-platform feature status
├── CLAUDE.md            ← project brief for AI sessions
├── .gitignore
├── server/              ← Go sync server (modular monolith)
│   ├── go.mod
│   ├── go.sum
│   ├── main.go          ← thin wiring: open DB, register handlers, listen
│   └── internal/
│       ├── config/      ← port, DB path, JWT secret, feature flags
│       ├── db/          ← SQLite open + migrations
│       ├── auth/        ← register, login, JWT middleware
│       ├── users/       ← user model, public key storage
│       ├── notes/       ← CRUD REST + per-note WebSocket hub
│       ├── folders/     ← folder CRUD
│       └── hub/         ← WebSocket room management (keyed by note ID)
└── notes/
    ├── mac/             ← macOS (Swift + SwiftUI)
    │   ├── Package.swift
    │   └── Sources/Notes/
    │       ├── NoteApp.swift
    │       ├── AuthService.swift    ← JWT + Keychain
    │       ├── LoginView.swift
    │       ├── NoteListView.swift   ← sidebar: folders + notes
    │       ├── NoteEditorView.swift ← text editor (was ContentView)
    │       ├── NoteListViewModel.swift
    │       ├── NoteEditorViewModel.swift
    │       ├── LocalStore.swift     ← directory-based, per-note files
    │       ├── SyncService.swift    ← WS to /notes/{id}/ws?token=<jwt>
    │       └── CryptoService.swift  ← X25519 keypair, AES-GCM, key wrap
    ├── windows/         ← Windows (C# + WinUI 3)
    ├── linux/           ← Linux (C++ + GTK4)
    └── ios/             ← iOS — not started
```

---

## Roadmap

### Phase 1 — Single note POC ✅ COMPLETE
- [x] Go server — running on Raspberry Pi 3 B via systemd
- [x] macOS client — Swift + SwiftUI, runs via `swift run`
- [x] Windows client — C# + WinUI 3, distributed as xcopy-deployable folder
- [x] Linux client — C++ + GTK4, built with CMake
- [x] End-to-end validated: Mac, Windows, and Linux syncing over LAN simultaneously
- [ ] iOS client — deferred; shares logic with macOS, build after Phase 2 is stable

### Phase 2 — Notes feature: full POC (in progress)

These are developed in order — each phase blocks the next.

**Phase 2a — User accounts**
Server restructured as modular monolith (SQLite, `internal/` packages, JWT auth).
Clients gain login/register UI, JWT stored in platform credential store.

**Phase 2b — Multiple notes + folders**
Server: `notes` and `folders` tables, CRUD REST, per-note WebSocket hub.
Client: sidebar with folder tree, note list, note editor (refactored from single-note UI).

**Phase 2c — End-to-end encryption**
Server stores ciphertext only. Clients generate X25519 keypairs, encrypt notes with
AES-256-GCM, store note keys wrapped with ECDH-derived keys.
Echo guard switches from content comparison to timestamp comparison.

**Phase 2d — Note sharing**
Sharer unwraps note key, re-wraps it with recipient's public key, uploads to server.
Recipient downloads their key envelope, decrypts independently.
Server enforces access control via `note_shares` table.

### Phase 3 — Conflict resolution
Replace last-write-wins with operational transforms or CRDTs for concurrent edits.

### Phase 4 — Files, photos, contacts, calendar
Expand the sync protocol to handle binary blobs, structured records, and calendar events.
The server becomes a full personal-cloud backend.
Each feature is a module enabled by config at startup.

### Phase 5 — Production deployment
HTTPS/WSS, dynamic DNS or relay service, proper packaging (`.app`, `.msix`, `.deb`/`.rpm`/flatpak).

---

## Decisions log

| Decision | Rationale |
|----------|-----------|
| One server, many native clients | Native feel on every platform is non-negotiable. Shared UI (web) would compromise it. |
| Go for the server | Excellent concurrency, single static binary, trivial to cross-compile and deploy. |
| gorilla/websocket | De-facto standard WebSocket library for Go; well-maintained. |
| Last-write-wins for POC | Simplest correct merge strategy. Good enough for single-user, single-note. Replace in Phase 3. |
| Unix milliseconds for timestamps | Sufficient resolution; avoids second-level collisions on fast typists; integer comparison. |
| camelCase local / snake_case wire | Local format matches Swift/JSON conventions. Wire format matches Go/server conventions. Decided early and kept consistent across all clients. |
| libsoup-3 for Linux WebSocket | GNOME-native, integrates with GLib main loop, no extra event loop needed alongside GTK4. |
| json-glib for Linux JSON | Same GNOME stack as GTK4 + libsoup; no extra dependency. |
| Modular monolith over microservices | Self-hosters run one binary, not a container orchestra. Internal package boundaries give the same isolation as services; can be extracted later if needed. |
| SQLite via modernc.org/sqlite | Pure Go (no CGO), cross-compiles to ARM for Raspberry Pi without a C toolchain. Single database file, zero config. |
| X25519 + AES-256-GCM for E2E encryption | X25519 is the modern standard for ECDH (simpler API and no cofactor attack surface vs P-256). AES-256-GCM provides authenticated encryption in one primitive. Both available in Apple CryptoKit without external dependencies. |
| Note key per note, wrapped per user | Sharing requires that collaborators decrypt independently. Re-wrapping the note key (not re-encrypting the note content) for each new collaborator is O(1) in ciphertext size regardless of note size. |
| Echo guard by timestamp, not content | With E2E encryption, two encryptions of the same plaintext produce different ciphertext (random AES-GCM nonce). Content equality comparison would always treat own updates as new and cause an infinite loop. |
| JWT via query param for WebSocket auth | HTTP headers are not reliably transmitted during WebSocket upgrade from all client environments. Query parameter is universally supported. |
| WinAppSDK: no self-contained native bundling | `WindowsAppSDKSelfContained=true` bundles WinAppSDK native DLLs that are incompatible with Windows Insider Preview builds (CoreMessagingXP.dll version mismatch). Removed; app relies on the installed Windows App Runtime instead. Users get an install prompt on first run on a new machine — acceptable tradeoff. |
| WinAppSDK 1.8 (not 1.6) | WinAppSDK 1.6 bootstrap failed on Windows Insider due to CBS package identity mismatch. 1.8 is the current stable release and was already installed on the dev machine. |
| `WindowsAppSdkBootstrapInitialize=true` explicit | WinAppSDK's build targets auto-disable bootstrap initialization when `WindowsAppSDKSelfContained=true` is set. Since we removed that flag, bootstrap init must be forced on explicitly, otherwise the app crashes silently (STATUS_FAIL_FAST_EXCEPTION) before XAML loads. |
| PRI files must be in publish folder | Without `WindowsAppSDKSelfContained`, the WinUI resource files (Amadeuz.pri, Microsoft.UI.pri, etc.) are not copied to the publish output automatically. Added a custom MSBuild target `CopyPriFilesToPublish` in the csproj to fix this. |
| Windows publish via VS MSBuild, not dotnet CLI | `dotnet publish` fails on WinUI 3 projects because PRI generation (`ExpandPriContent` task) requires VS-installed tools not present in the dotnet SDK. Use `MSBuild.exe` from Visual Studio 2022. |
| Phase 2b before 2a (notes/folders before auth) | Building auth before the data model is validated means building auth for a schema that might change. Notes and folders are the product. Auth is plumbing. Validate the data model end-to-end first, then layer auth on top. |
| Single `/ws` endpoint for Phase 2b (no REST yet) | The planned architecture uses per-note WebSocket rooms. For the interim multi-note phase (no auth), a single `/ws` endpoint with typed messages is simpler and sufficient. REST + per-note WS will be introduced when auth is implemented. |
| Synchronous persist, atomic write (rename) | Async `go persist()` goroutines race: an older goroutine can overwrite the file with a stale snapshot after newer data has already been written. Synchronous writes eliminate the race. `os.Rename` over a temp file ensures no partial-write corruption if the process is killed mid-write. |
| Persist race caught by unit test | `TestPersistAndReload` failed with `unexpected end of JSON input` when persist was async — an older goroutine was winning the write after the test's synchronous call. Making persist synchronous made the test deterministic and the bug disappeared. Tests earned their keep on day one. |
