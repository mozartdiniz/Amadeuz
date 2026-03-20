# Amadeuz — Vision & Architecture

## What we're building

An open-source, self-hosted alternative to iCloud, starting with a **Notes app**.
The end goal is a full personal-cloud suite: notes, files, photos, contacts, calendar — all
owned by the user, synced across their devices, running on their own infrastructure.

We are starting with the smallest useful slice: a single synced note that works on every
major platform.

---

## Core principles

### 1. Native, always (with one pragmatic exception)
No browser engine wrappers. No Electron, no Tauri, no WebView containers.
Every client is written in the idiomatic language and UI framework for its platform.
Users should not be able to tell this isn't a platform-first app.

| Platform | Language / Framework |
|----------|---------------------|
| macOS    | Swift + SwiftUI     |
| iOS      | Swift + SwiftUI     |
| Android  | Kotlin + Jetpack Compose |
| Windows  | Rust + GTK4 + Libadwaita *(same codebase as Linux)* |
| Linux    | Rust + GTK4 + Libadwaita |
| Server   | Go                  |

Multiple platform-specific codebases are acceptable and expected.
Shared code is good when it doesn't compromise native feel; it is never worth forcing.

**Windows:** After WinUI 3 and WPF rewrites both failed (packaging issues, toolchain friction),
and with Electron never getting off the ground, the pragmatic decision was made to compile the
Linux GTK4 app for Windows. GTK4 runs natively on Windows via gvsbuild. The same Rust codebase
serves both Linux and Windows — resources are embedded at compile time on Windows, and a
PowerShell script (`build-windows.ps1`) handles the full build and DLL collection. This is
consistent with the "native" principle: GTK4 is a real native toolkit, not a web engine wrapper.

### 2. Offline-first
Every client loads from local disk immediately on startup and works fully without a server.
The server is a sync accelerator, not a dependency for basic use.

### 3. Minimal protocol surface
The wire protocol between client and server is intentionally small and stable.
New features should extend it without breaking existing clients.

### 4. Self-hosted by default
The server runs wherever the user wants: a Raspberry Pi, a home NAS, a VPS.
No vendor lock-in. No accounts unless the user explicitly enables them.

### 5. End-to-end encrypted by default (Phase 2c)
The server is a routing and storage layer, not a trust boundary. Note content will be
encrypted on the client before transmission. The server stores and relays ciphertext only.
The server operator cannot read user data even with full database access.
*(Currently implemented: plaintext storage with JWT auth. E2E encryption is Phase 2c.)*

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
       │                             │     blobs / db               │
  local disk                         └──────────────────────────────┘
  (per device)
```

**The server** is a routing and storage layer. It enforces who can access what via JWT auth.
All mutations go through REST; per-note WebSocket is reserved for live keystroke streaming only.

**Each client** owns a local copy of the data. On connect, it runs a full sync: merges server
state with local state by timestamp (last-write-wins), pushes any locally-created or
locally-newer items. JWT is stored in the platform's secure credential store (Keychain on
Apple, Electron `safeStorage` (DPAPI-backed) on Windows, libsecret on Linux).

---

## Wire protocol (current — Phase 2a)

All messages are JSON. `updated_at` is Unix milliseconds.

### REST endpoints (authenticated via `Authorization: Bearer <token>`)

```
POST /auth/register           body: { email, password }                     → { token, recovery_code }
POST /auth/login              body: { email, password }                     → { token }
POST /auth/recover            body: { email, recovery_code, new_password }  → { token, recovery_code }

GET    /folders               → { folders: [...] }
POST   /folders               body: { id?, name, created_at? }              → { folder }
PATCH  /folders/:id           body: { name }                                → { folder }
DELETE /folders/:id                                                         → 204

GET    /notes                 → { notes: [...] }  (includes content)
POST   /notes                 body: { id?, folder_id?, title, content, updated_at, created_at? } → { note }
PATCH  /notes/:id             body: { title, content, updated_at }          → { note } or 409 if stale
PATCH  /notes/:id/move        body: { folder_id }                           → { note }
DELETE /notes/:id                                                           → 204

PUT    /blobs/:id             (authenticated)  → 201 created / 200 already exists
GET    /blobs/:id             (unauthenticated) → blob data
```

JWT is also accepted as `?token=<jwt>` query param (for WebSocket upgrade, where HTTP headers
are not reliably supported by all client environments).

### WebSocket (per note, authenticated via `?token=<jwt>`)

```
GET /notes/:id/ws?token=<jwt>
```

```json
// Server → client on connect
{ "type": "init", "title": "...", "content": "...", "updated_at": 123 }

// Client → server on keystroke
{ "type": "update", "title": "...", "content": "...", "updated_at": 123 }

// Server → all other clients watching this note
{ "type": "update", "title": "...", "content": "...", "updated_at": 123 }
```

Server applies last-write-wins: only accepts and rebroadcasts if `updated_at` is strictly
greater than stored. Ping/pong keepalive: 30 s interval, 60 s read deadline.

### Phase 2c wire protocol (planned — E2E encryption)

When E2E encryption is implemented, `content` fields will carry base64url-encoded
AES-256-GCM ciphertext (random nonce prepended). Additional endpoints will be added for
key envelope exchange and note sharing. The REST + per-note WebSocket structure is unchanged;
only the `content` field changes from plaintext to ciphertext. Echo guard will use timestamp
comparison only (not content comparison) because two encryptions of the same plaintext
produce different ciphertext due to the random AES-GCM nonce.

---

## Local storage format

Each client stores all folders and notes in a single JSON file.

```json
{ "folders": [...], "notes": [...] }
```

Storage paths:

| Platform | Data file | Credential store |
|----------|-----------|-----------------|
| macOS    | `~/Library/Application Support/amadeuz/data.json` | Keychain |
| Windows  | `%APPDATA%\amadeuz\data.json` | Electron `safeStorage` (DPAPI-backed) |
| Linux    | `~/.local/share/amadeuz/data.json` | libsecret / GNOME Keyring |
| iOS      | `<App>/Library/Application Support/amadeuz/data.json` | Keychain |
| Server   | `amadeuz.db` (SQLite) | — |

---

## Sync logic

**On login / reconnect (`fullSync`):**
```
1. Load local state from disk
2. GET /folders + GET /notes from server
3. For each server item: if server.updatedAt > local.updatedAt → accept, overwrite local
4. For each local item not on server, or local.updatedAt > server.updatedAt → push to server
5. Delete local items that server no longer has (were deleted by another client)
```

**On user keystroke (debounced 500 ms):**
```
save locally → PATCH /notes/:id (with current updatedAt)
if 409 stale → server has newer; re-fetch and discard local change
```

**Per-note WebSocket (live streaming):**
```
open when note is selected, close when note is deselected
receive "init" on connect → apply same timestamp merge as fullSync
receive "update" → apply if updated_at > local
send "update" on debounced keystroke (in addition to REST PATCH)
```

---

## Repository layout

```
amadeuz/
├── VISION.md            ← this file
├── PROGRESS.md          ← per-platform feature status
├── DIARY.md             ← build journal
├── CLAUDE.md            ← project brief for AI sessions
├── .gitignore
├── server/              ← Go sync server (modular monolith)
│   ├── go.mod
│   ├── go.sum
│   ├── main.go          ← thin wiring: open DB, register handlers, listen, reset-password CLI
│   ├── server_test.go   ← 25 integration tests
│   └── internal/
│       ├── db/          ← SQLite open, migrations, JWT secret, NewID
│       ├── auth/        ← register, login, recover handlers + JWT middleware
│       ├── notes/       ← note CRUD REST + per-note WebSocket hub
│       ├── folders/     ← folder CRUD handlers
│       └── blobs/       ← blob upload/download handlers
└── notes/
    ├── mac/             ← macOS (Swift + SwiftUI)
    │   ├── Package.swift
    │   └── Sources/Notes/
    │       ├── NoteApp.swift        ← @main entry; @StateObject vm; Sign Out menu command
    │       ├── Models.swift         ← Folder, Note, AuthResponse, NoteWsMsg types
    │       ├── ContentView.swift    ← auth gate; NavigationSplitView; recovery code sheet
    │       ├── NoteViewModel.swift  ← @MainActor; auth state; fullSync(); REST CRUD
    │       ├── MarkdownEditor.swift ← NSTextView-based rich editor (Markdown + inline images)
    │       ├── BlobStore.swift      ← local blob cache, pending upload queue, authenticated PUT
    │       ├── LocalStore.swift     ← read/write data.json in Application Support
    │       ├── SyncService.swift    ← NoteSync: per-note WebSocket, receive-only, auto-reconnect
    │       ├── KeychainStore.swift  ← save, load, delete JWT from macOS Keychain
    │       ├── APIClient.swift      ← all REST calls + WebSocket URL builder
    │       └── AuthView.swift       ← Login / Register / Recover UI; RecoveryCodeView sheet
    ├── windows/         ← Windows (C# + WinUI 3) — legacy, preserved for reference
    ├── windows-wpf/     ← Windows (C# + WPF + ModernWpfUI) — legacy, preserved for reference
    ├── windows-electron/ ← Windows (Electron) — active client (in progress)
    ├── linux/           ← Linux (C++ + GTK4)
    ├── ios/             ← iOS (Swift + SwiftUI)
    └── android/         ← Android (Kotlin + Jetpack Compose)
```

---

## Roadmap

### Phase 1 — Single note POC ✅ COMPLETE
- [x] Go server — running on Raspberry Pi 3 B via systemd
- [x] macOS client — Swift + SwiftUI, runs via `swift run`
- [ ] Windows client — Electron rewrite in progress (WPF and WinUI 3 versions preserved as legacy)
- [x] Linux client — C++ + GTK4, built with CMake
- [x] End-to-end validated: Mac, Windows, and Linux syncing over LAN simultaneously
- [x] iOS client — Swift + SwiftUI, shares `LocalStore`/`SyncService` with macOS, running on device
- [ ] Android client — Kotlin + Jetpack Compose, code complete, pending first device run

### Phase 2 — Notes feature: full POC (in progress)

These are developed in order — each phase blocks the next.

**Phase 2a — User accounts** ✅ COMPLETE (server + macOS)
Server restructured as modular monolith (SQLite, `internal/` packages, JWT auth, REST API,
per-note WebSocket). macOS client updated with login/register/recover UI, JWT in Keychain,
REST CRUD, per-note WebSocket live sync. 25 integration tests passing.
Windows, Linux, iOS, Android: Phase 2b architecture still in use — Phase 2a parity is the next step.

**Phase 2b — Multiple notes + folders** ✅ COMPLETE (server + macOS + Windows + Linux + iOS)
Server: folder + note CRUD over typed WebSocket messages, in-memory store + data.json.
Client: three-column layout (folder sidebar, note list, note editor) on all three desktop platforms.
macOS also has: inline images, Markdown styling, offline blob queue, search, move note.
Windows (updated March 2026): REST + per-note WS architecture replacing the old shared-WS protocol.
Now has auth, trash, move note, search, sign out — full parity with macOS except Markdown/images.

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

## Linux client: fresh start (decided March 2026)

The C++ GTK4 client was brought to full Phase 2a feature parity (auth, REST, per-note WebSocket,
search, move, Markdown, inline images). That code is preserved in `legacy-code/linux/`.

The decision was made to discard it and start fresh with the modern GNOME stack before any
further feature work. The C++ codebase was always intended as a proof-of-concept to validate
the feature set, not a long-term foundation. Now that the feature set is validated, the right
move is to build on the stack that the GNOME ecosystem is converging on.

### Chosen stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Language | **Rust** | Memory safety, native `async/await` for sync, first-class gtk4-rs bindings |
| Toolkit | **GTK4 + Libadwaita** | Required for modern GNOME look; `AdwNavigationSplitView` for adaptive layout |
| UI definition | **Blueprint** `.blp` files | ~70% shorter than GtkBuilder XML; compiles to standard `.ui` at build time |
| Build system | **Meson + Cargo** | GNOME standard; handles Blueprint compilation and GResource bundling |
| Config storage | **GSettings** | Platform-native; avoids custom JSON settings file |
| Credential store | **libsecret** (via `libsecret-rs`) | Same as C++ client; GNOME Keyring integration |
| Async runtime | **Tokio** | `tokio-tungstenite` for WebSocket; `reqwest` for REST |
| Local data | **Flat JSON file** (`data.json`) | Matches all other clients; sufficient for POC scale |

### Key widgets

| Component | Widget |
|-----------|--------|
| Main window | `AdwApplicationWindow` |
| Folder sidebar + note list | `AdwNavigationSplitView` (adaptive — works on phone too) |
| Note rows | `AdwActionRow` |
| Header | `AdwHeaderBar` |
| Toast notifications | `AdwToast` |
| Empty state | `AdwStatusPage` |

### Primary reference project

**Fragments** (GNOME torrent client) — Rust + GTK4 + Libadwaita + Meson. This is the
structural reference for the Rust code organisation, GObject model patterns, and Meson build.

Note: `iotas` (GNOME notes app, also studied) is **Python**, not Rust. Its architecture
patterns are not directly applicable.

### Scope for the initial build (Phase 2a parity)

Reach functional parity with the legacy C++ client. Deferred until after parity:
- i18n / gettext (add later without restructuring anything)
- Flatpak manifest (add when packaging becomes relevant)
- GNOME Shell Search Provider (post-POC feature)

### Source layout

```
notes/linux/
├── meson.build
├── Cargo.toml
├── data/
│   ├── com.amadeuz.Notes.gschema.xml   ← GSettings schema
│   └── ui/                             ← Blueprint .blp files
└── src/
    ├── main.rs
    ├── app.rs
    ├── model/         ← GObject data models (Note, Folder)
    ├── ui/            ← AdwWindow, sidebar, editor, widgets
    └── backend/       ← local_store, sync_worker, keyring
```

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
| X25519 + AES-256-GCM for E2E encryption (Phase 2c) | X25519 is the modern standard for ECDH (simpler API and no cofactor attack surface vs P-256). AES-256-GCM provides authenticated encryption in one primitive. Both available in Apple CryptoKit without external dependencies. |
| Note key per note, wrapped per user (Phase 2c) | Sharing requires that collaborators decrypt independently. Re-wrapping the note key (not re-encrypting the note content) for each new collaborator is O(1) in ciphertext size regardless of note size. |
| Echo guard by timestamp, not content (Phase 2c) | With E2E encryption, two encryptions of the same plaintext produce different ciphertext (random AES-GCM nonce). Content equality comparison would always treat own updates as new and cause an infinite loop. |
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
| ObservableCollection diff instead of clear+rebuild (Windows) | Clearing `FilteredNotes` clears the ListView selection, which cascades into `SelectionChanged` → `NoteSelectionChanged` → editor wipe. Maintaining the collection via a diff (remove missing items, insert/`Move()` others) keeps selection intact through folder switches, syncs, and note creates. `Move()` fires `NotifyCollectionChangedAction.Move` which the ListView handles without touching selection. |
| `INotifyPropertyChanged` on `Note` (Windows) | In-place property updates (`Title`, `Content`, `UpdatedAt`) via `INotifyPropertyChanged` refresh the ListView row without removing and re-inserting the item. Same motivation as the ObservableCollection diff: selection is never disturbed. |
| Callbacks injected into `NotesViewModel` (Windows) | WinUI 3 data binding works for the folder/note `ObservableCollection`s. The editor requires synchronous, ordered updates tied to flush-on-switch logic, which two-way binding would fight. Explicit callbacks (`onEditorChanged`, `onConnectionChanged`, `onNoteAutoSelected`) injected at construction make the data flow clear and avoid binding surprises. |
| `FolderItem` sentinel for "All Notes" (Windows) | A single `ListView` needs to show both "All Notes" and real folders. A flat `FolderItem` class with a fixed sentinel ID (`"__all__"`) lets XAML treat all rows uniformly while the view model uses the sentinel to decide filter vs show-all. The sentinel's `IsAllNotes` property lets DataTemplates hide rename/delete controls. |
| Full rebuild vs diff for Linux `GtkListBox` | GTK4's `GtkListBox` has no equivalent of `ObservableCollection.Move()`. Rather than implement a manual diff with `gtk_list_box_remove`/`gtk_list_box_insert` that replicates the Windows logic in C, the simpler approach is: suppress selection signals → remove all rows → re-add → call `gtk_list_box_select_row` to restore. Selection loss during rebuild is invisible to the user because signals are suppressed throughout. Correct and easy to reason about at this scale. |
| `GtkGestureClick` + `GtkPopover` for context menus (Linux) | GTK4 removed `GtkMenu`. The replacement for right-click menus is `GtkGestureClick` (button=3) attached to each row, with data stored on the gesture via `g_object_set_data`. On press, a `GtkPopover` with frameless buttons is created, parented to the row, and shown. Unparented in `GtkPopover::closed` to avoid leaks. |
| `GtkStack` for editor empty/active state (Linux) | When no note is selected, a "Select a note" placeholder should be shown instead of the editor. `GtkStack` with named pages ("empty" / "editor") is the idiomatic GTK4 approach — cleaner than showing/hiding individual widgets. |
| `GTK_EVENT_CONTROLLER()` cast required (Linux) | `GtkGesture` is a subclass of `GtkEventController`, but the incomplete-type forward declaration in the GTK4 headers prevents an implicit conversion. `GTK_EVENT_CONTROLLER(gesture)` macro is required when calling `gtk_widget_add_controller`. |
| Kotlin + Jetpack Compose for Android | Kotlin is Google's first-class Android language (Java is legacy). Jetpack Compose is the modern declarative UI toolkit — similar mental model to SwiftUI. MVVM + ViewModel + StateFlow is the Google-recommended architecture. No alternative seriously considered. |
| OkHttp for Android WebSocket | De facto standard for HTTP/WebSocket on Android. Simpler API than `java.net.http` (added in API 21 but WebSocket support is limited). Well-maintained, Kotlin-idiomatic. |
| Gson for Android JSON | Simpler than `kotlinx.serialization` (requires no Kotlin compiler plugin). No custom serializer needed — Gson omits null fields by default, which is exactly what the wire protocol requires. |
| Handler.postDelayed for Android reconnect | OkHttp WebSocket callbacks arrive on OkHttp dispatcher threads. Reconnect needs a delay before retrying. `Handler(Looper.getMainLooper()).postDelayed()` is the simplest correct solution — no coroutine scope needed in SyncService, no Thread.sleep blocking a dispatcher thread. |
| Navigation drawer instead of three-column layout (Android) | Persistent three-column layouts are not appropriate for portrait phone screens. The drawer pattern is the Android-native equivalent: swipe or tap the hamburger to reveal folders, tap a note to open the editor full-screen. On large screens (tablets), a future iteration could use `NavigationRail` or `PermanentNavigationDrawer`. |
| AGP 9.x applies kotlin-android internally | Adding `org.jetbrains.kotlin.android` to `plugins {}` in AGP 9.x causes "Cannot add extension 'kotlin'" — the plugin is already applied by the Android Gradle Plugin. The `kotlin.compose` plugin (Compose compiler) must still be added explicitly. |
| Notes do not require a folder | Folders are an optional organisational tool, not a prerequisite for taking a note. Requiring a folder selection before creating a note adds friction. Unfoldered notes appear in "All Notes" and can be moved into a folder at any time. The `folder_id` field on Note is an empty string (not nullable) for simplicity — no sentinel value, no nullable type plumbing across all clients. |
| `move_note` as a dedicated message type | Moving a note changes `folder_id` but not `title` or `content`. Using `update_note` for a move would be incorrect: it would require the client to send the full note content just to change the folder, and the last-write-wins timestamp guard could reject a move if a concurrent content update came in with a newer timestamp. A dedicated `move_note` message is semantically clear and avoids that ambiguity. |
| Search as client-side filter, not server query | At this stage (no auth, single user, all notes in memory), filtering on the client is instant and requires no server changes. The search field simply filters `notesInSelectedFolder` by lowercased substring match on title and content. Server-side full-text search is deferred to Phase 2a+ when the data model is larger and per-user isolation is in place. |
| Client-generated IDs for offline creation | Notes and folders are created locally with a UUID generated on the client. The entity appears in the UI immediately and is saved to disk. The ID is included in the `create_folder`/`create_note` message sent to the server. The server uses the client-provided ID if present, generates one otherwise. On reconnect, `handleInit` pushes any local-only entities to the server using those same IDs — no conflicts, no dropped data. The alternative (server-generated IDs) required a round-trip to the server before the entity could appear, making offline creation impossible. |
| Row height fixed by always showing folder label | When a note has no folder, hiding the folder label row caused the list row to be shorter. Moving the note to a folder made the label appear — but SwiftUI caches row heights and would crop the newly visible label. Fixed by always rendering the label and showing "—" when there is no folder. Constant height, no caching surprise. |
| `loadNoteIntoEditor` called directly in `createNote`, not via `onChange` | SwiftUI's `onChange(of:)` only fires when the observed value changes while the view is already in the hierarchy. When `notesInSelectedFolder` was empty (no notes in the folder), the `List` was replaced by `ContentUnavailableView`. Creating the first note made both the `List` and its selection appear in the same render cycle — `onChange` never fired, the editor was never loaded, and it showed the previous note's content. Fix: call `loadNoteIntoEditor` synchronously in `createNote()` before setting `selectedNoteID`. `onChange` remains as a secondary trigger for user-driven selection changes and is idempotent when called twice. |
| Markdown as note content format, with `amadeuz://blob/uuid` for images | Note content is plain Markdown text. Images are referenced as `![](amadeuz://blob/<uuid>)` — a custom URI scheme that identifies a blob by its UUID. This keeps notes as plain strings (easy to persist, sync, and search), while the blob store handles the binary data separately. Base64 embedding was rejected (inflates note size, breaks WebSocket framing for large images). A block-level document model (like NSTextStorage-native) was rejected (too complex for cross-platform parity). |
| Client-generated blob IDs, `PUT /blobs/:id` (idempotent) | Images follow the same offline-first pattern as notes and folders: the client generates a UUID, saves the blob to local disk immediately, inserts the image into the note with the final ID, and queues the blob for upload. On reconnect, `uploadPending()` flushes the queue via `PUT /blobs/:id`. The server accepts a client-provided ID and is idempotent — if the blob already exists, it returns 200 without re-writing. This makes retries after partial failure safe. |
| Pending blob queue persisted to `pending_blobs.json` | The set of blob IDs not yet uploaded to the server is persisted to disk alongside the blob cache. If the app restarts while offline, the pending set survives and `uploadPending()` is called again on the next reconnect. The pattern mirrors the note/folder offline-first design. |
| `noteSelectionChanged` guards `old == editingNoteID` before flushing | `createNote()` calls `loadNoteIntoEditor(newNote)` (setting `editingNoteID` to the new note) before setting `selectedNoteID`. SwiftUI's `onChange` then fires `noteSelectionChanged(from: oldNoteID, to: newNoteID)`. Without a guard, this would call `flushNote(oldNoteID, "", "")` — overwriting the old note's content with empty string — because the editor's `editingTitle`/`editingContent` had already been cleared by `loadNoteIntoEditor`. The guard `old == editingNoteID` makes the flush conditional on the editor still displaying the old note, which it no longer is after `loadNoteIntoEditor` has run. |
| Markdown styling via NSTextStorage attribute manipulation, not content mutation | `applyMarkdownStyling()` adds visual attributes (font size, color, strikethrough) to the `NSTextStorage` without changing the underlying characters. `extractMarkdown()` serialises only `.attachment` attributes — it ignores all visual attributes. This means the Markdown string round-trips cleanly regardless of styling applied, and there is no risk of styling code corrupting note content. |
| Per-keystroke styling scoped to current paragraph only | Calling `addAttribute` over the full document range on every keystroke causes `NSLayoutManager` to invalidate the entire layout, which forces a scroll jump as the layout reflows. Fix: `applyMarkdownStylingForCurrentLine()` computes `paragraphRange(for: cursorPosition)` and invalidates only that range. Full-document styling (`applyMarkdownStyling()`) is reserved for note load and paste operations where a full pass is correct. |
| No email integration for password recovery | Email requires SMTP infrastructure (or a third-party email service), which is a self-hosting burden and an external dependency. The target audience (privacy-conscious, self-hosters) is exactly the audience least likely to want to configure SMTP or trust a third party with account metadata. |
| Recovery code at registration (no email) | On account creation the server generates a one-time recovery code (high-entropy random string). The user saves it (password manager, printed paper). Presenting the code resets the password. CLI admin reset (`amadeuz-server reset-password <email> <new-password>`) is available as a last resort. This is the correct model for a self-hosted tool — no external dependency, honest about the tradeoffs. |
| REST for CRUD, WebSocket for live streaming only | REST is stateless, maps cleanly to CRUD semantics, works with standard HTTP tooling, and lets clients queue mutations for offline retry. WebSocket is reserved for real-time per-keystroke streaming — the one case where request/response latency would be noticeable. |
| Per-note WebSocket rooms (not a single bus) | A single WS endpoint that broadcasts all events to all connected clients requires every client to filter noise. Per-note rooms mean the server only sends a client updates for the note it currently has open. Scales better and eliminates unnecessary data transfer. |
| JWT stored in macOS Keychain | Platform-native secure storage. Survives app restart without the user re-authenticating. Keychain items are sandboxed per app. No plaintext credentials in UserDefaults or on disk. |
| `applyToken` clears local state before fullSync | When a user logs in or recovers, any in-memory and on-disk state from a previous session must be wiped before syncing from the server. Without this, the merge logic would push the previous user's notes to the new account. Discovered during testing. |
| Recovery code rotation on use | A recovery code that can be used multiple times is equivalent to a second password that never changes. Rotating the code on each use limits the window of exposure. If a code is compromised and used by an attacker, the legitimate user's next recovery attempt will fail — which is a signal that the code was leaked. |
| `APIError.conflict` carries server message | The server sends descriptive plain-text error bodies (e.g. "email already registered" vs "stale update"). A generic "Conflict" message loses this information. Passing the body through means the UI shows the server's actual explanation without needing to duplicate error string logic on the client. |
| E2E encryption is non-negotiable for internet-facing deployments | On a LAN the user owns the trust boundary. On a VPS the hosting provider has root access to the SQLite file. Without E2E, a user who moves off iCloud to avoid Apple reading their notes has traded one corporation for their VPS provider. E2E ensures the server stores only ciphertext regardless of where it runs. This is also the primary reason the privacy-conscious audience will trust the product with sensitive content (medical notes, private journals, passwords). |
| `GtkDropTarget` + `GtkTextChildAnchor` + `GtkPicture` for Linux inline images | `GtkDropTarget` with `GDK_TYPE_FILE_LIST` intercepts file drops on `GtkTextView`. MIME type is checked via `g_file_query_info` before accepting. Each image is given a UUID, copied to the blobs directory, and inserted as a `GtkTextChildAnchor` with a `GtkPicture` child widget (scaled to max 400 px wide). `GtkPicture` is used over `GtkImage` because it renders the full image at the requested pixel dimensions; `GtkImage` treats non-icon paintables with no natural size and requires an explicit size request. A `blob_anchors_` map on `MainWindow` tracks anchor → UUID so `cb_content_changed` can serialize the buffer back to `![](amadeuz://blob/<uuid>)` Markdown by iterating character-by-character with `gtk_text_iter_get_child_anchor`. On note load, `render_blob_images()` does the reverse: finds all `![](amadeuz://blob/<uuid>)` patterns in reverse offset order, deletes each text span, and inserts an image anchor in its place. |
| `NotesViewModel` constructor fires `on_auth_state_` before widgets exist (Linux) | The VM is constructed before any GTK widgets, so the initial `on_auth_state_(true)` callback (triggered when a saved token is found in the keyring) calls `show_main_page()` while `root_stack_` and all other widget pointers are still null — a silent no-op. `GtkStack` defaults to the first page added ("auth"). Fixed by explicitly calling `show_main_page()` or `show_auth_page()` after all widgets are built, replacing the one-sided `if (!is_logged_in())` fallback with a full `if/else` branch. |
| Linux client restarted in Rust + Libadwaita (March 2026) | The C++ GTK4 client reached full Phase 2a feature parity. Rather than continue adding features to a C++ codebase that was always a proof-of-concept, the decision was made to restart with Rust + GTK4 + Libadwaita + Blueprint + Meson — the stack that GNOME Circle apps are converging on. The feature set was already validated; the restart is a code quality decision, not a product decision. Legacy C++ code preserved in `legacy-code/linux/`. |
| Fragments (not iotas) as the primary Rust reference for Linux | Both were studied as GNOME Circle reference projects. iotas turned out to be Python (PyGObject), not Rust — its GObject patterns look different and are not directly transferable. Fragments is a mature Rust + GTK4 + Libadwaita + Meson app with clean code organisation, making it the correct structural reference. |
| Flat JSON file for Linux local storage (not SQLite) | SQLite would be the right choice for a multi-user or large-scale app. For the POC, a single `data.json` file matches every other client and keeps the data layer simple and consistent. Can be migrated to SQLite later without changing the sync or UI layers. |
| GSettings for config, not custom JSON (Linux) | GSettings is the platform-native config mechanism — values are manageable via `dconf-editor`, follow system-wide patterns, and don't require hand-rolling serialisation. The custom `settings.json` from the C++ client is replaced. |
| Server-side `deleted_at` column for trash (not synthetic folder_id) | The first instinct was to move a trashed note to `folder_id = "__wastebasket__"`. The server rejected this with an FK violation — `folder_id` is a foreign key and `__wastebasket__` doesn't exist in the `folders` table. Rather than create a real wastebasket folder in the DB, a nullable `deleted_at INTEGER` column was added to `notes`. The client maps `deleted_at IS NOT NULL` ↔ `folder_id = "__wastebasket__"` at the load/sync boundary. The server exposes `PATCH /notes/:id/trash` and `PATCH /notes/:id/restore` (both 204). |
| `pragma_table_info` for idempotent schema migration | `ALTER TABLE … ADD COLUMN` fails if the column already exists. A `SELECT COUNT(*) FROM pragma_table_info('notes') WHERE name='deleted_at'` check gates the ALTER, making migrations safe to re-run on existing production databases. |
| Inline auth navigation links instead of `Gtk.StackSwitcher` tabs | A `StackSwitcher` showed three tabs ("Login", "Register", "Recover"). In testing, the Recover tab was never found — users don't associate a top tab with password recovery. Replaced with contextual inline `flat` buttons: "Create account" and "Forgot your password?" beneath the Sign In form; "Already have an account?" on Register; "Back to sign in" on Recover. Wired in `AmzAuthView::ObjectImpl::constructed()` — the view wires its own internal navigation independently of the window. |
| Auth button state reset on page navigation | `set_sensitive_all(false)` fires when any auth request starts. If the user navigates to a different auth page while a request is in-flight, the new page's button inherits the disabled state. Fixed by connecting to `auth_stack.connect_visible_child_notify`: re-enable all buttons and clear the error label on every page transition. |
| Welcome screen onboarding (offline vs. connect) | On first launch with no saved token, users landed directly on the auth screen. Changed to a dedicated welcome page (`Adw.StatusPage` with two `Adw.ActionRow`s): "Use Offline" and "Connect to Server". The choice is persisted — `offline_mode: true` in `data.json` skips the welcome screen on subsequent launches. The `win.sign-out` action checks `offline_mode`: if true, it clears the flag and returns to welcome without a destructive confirmation (no data is lost). |
| Dynamic `gio::Menu` model instead of static Blueprint menu | The hamburger menu needs different items based on state: authenticated → "Server Settings" + "Sign Out"; offline → "Return to Start". Updating labels in a static Blueprint-defined menu is awkward. Instead, `refresh_ui()` calls `menu_button.set_menu_model(Some(&menu))` with a freshly built `gio::Menu` on each state change. Cost is negligible (only rebuilds on auth/offline transitions). |
| `gdk::Texture` clipboard branch for image paste | The original paste handler only handled `gdk::FileList` (files from file manager). Screenshots (PrintScreen) and images copied from browsers place a `GdkTexture` on the clipboard. Added a second branch: `clipboard.read_texture_async()` saves the texture as PNG to the note's local image directory. The callback type is `Result<Option<Texture>, glib::Error>` — `None` means no image was available, not an error. |
| `Blueprint StackPage { name: … }` vs widget `name:` property | Setting `name: "foo"` on a direct child of `Gtk.Stack` in Blueprint sets `GtkWidget.name` (used for CSS targeting), not the `GtkStackPage` name that `set_visible_child_name()` looks up. Must wrap children in `Gtk.StackPage { name: "foo"; child: … }`. Discovered via "Child name 'X' not found in GtkStack" runtime warnings. |
| Windows client: rewritten from shared-WS to REST + per-note WS (March 2026) | The original Windows client used a custom shared WebSocket protocol at a single `/ws` endpoint (with typed messages like `create_folder`, `update_note`). The server dropped this in favour of standard REST + per-note WebSocket rooms (same architecture as all other clients). Rather than patch the old code, the Windows sync layer was fully rewritten: `ApiClient.cs` handles all REST calls + auth; `SyncService.cs` now wraps a per-note `ClientWebSocket` instead of a shared bus; `NotesViewModel.cs` was rewritten around `FullSync()` + `NoteSelectionChanged()`. |
| `ContentDialog` for recovery code display (Windows) | The initial design placed a recovery code panel inline in the auth overlay XAML. It was never visible: `StateChanged` (which hides the auth overlay) and `RecoveryCode` (which would show the panel) were both enqueued in the same `_dispatcher.TryEnqueue` batch — the overlay was already gone by the time the code tried to show the panel. Fix: display the recovery code in a `ContentDialog` created entirely in code-behind, shown immediately after auth succeeds (while the app is already in main-view state). |
| `NormaliseNote()` maps `deleted_at` → `folder_id = "__wastebasket__"` at load boundaries (Windows) | The server returns notes with `deleted_at` set and `folder_id = null` for trashed notes. The Windows client uses a single `FolderId` string to route notes to the correct ListView bucket. Rather than add trash-vs-active branches throughout the filtering and display code, `NormaliseNote()` is called at every load boundary (REST sync, local load, WS init) and sets `FolderId = "__wastebasket__"` when `DeletedAt` has a value. This keeps all filtering logic uniform. |
| `PasswordVault` (Windows Credential Manager) for JWT storage (Windows) | WinRT `PasswordVault` (Windows Credential Manager) is the platform-native secure storage for credentials — equivalent to Keychain on Apple and libsecret on Linux. Resource name: `amadeuz-jwt`; username: `jwt`. Survives app restart without re-authentication. No plaintext token on disk. |
| First line as title (Windows) — matching macOS/iOS | The original Windows editor had a separate `TextBox` for the title and a second `TextBox` for the body. macOS and iOS abandoned the dedicated title field: the note has a single body, and the first line is the title. Windows was updated to match: a single `RichEditBox` holds `title + "\n" + content`; `SplitBody()` extracts the two fields at flush time. This is both simpler (one editor, one binding) and consistent across clients (a note edited on Windows and viewed on macOS shows the same line as the title). |
| `RichEditBox` instead of `TextBox` for Windows note editor | `TextBox` supports only uniform character formatting. Markdown styling (different sizes for headings, bold, italic, strikethrough) requires per-character formatting, which is only available via `RichEditBox` and its `ITextDocument` / `ITextRange` API. `RichEditBox` internal paragraph separator is `\r` (not `\n`); `GetText` appends a trailing `\r\0`. `GetBody()` strips those and converts `\r` → `\n` for storage. `_suppressEditorChanged` and `_applyingFormat` guard flags prevent recursive `TextChanged` triggers during programmatic updates. |
| Markdown formatting debounce on Windows (120 ms, separate from save debounce) | Applying markdown formatting (`ApplyMarkdownFormatting`) on every single keystroke causes visible lag on large notes because `ITextRange.CharacterFormat` changes are synchronous and invalidate layout. A 120 ms `DispatcherTimer` (restarted on every `TextChanged`) defers the format pass until typing pauses. The save debounce remains at 500 ms and is wired separately via `Vm.OnEditorChanged`. The two timers are independent — neither blocks the other. |
| Wastebasket as a pinned button outside the folder `ListView` (Windows) | The Wastebasket sentinel was initially included in `FolderItems`, the same `ObservableCollection` that backs the folder `ListView`. This caused confusion: the Wastebasket row was indistinguishable from a real folder to the selection/deselection logic, and its "selected" state was lost whenever `RebuildFolderItems()` rebuilt the collection. Moving it to a dedicated XAML `Button` (pinned between the `ListView` and the "New Folder" button) solves both problems: `_wastebasketSelected` bool tracks its visual state independently; `AccentButtonStyle` is applied/cleared explicitly; the `ListView` only ever contains "All Notes" + real user folders. |
| Image paste as Ctrl+V intercept on `RichEditBox` (Windows) | `RichEditBox` has built-in Ctrl+V paste that pastes bitmaps as OLE objects embedded in the document. OLE objects are not extractable back to bytes through the public `ITextDocument` API, so there is no way to serialize them for sync. The solution is to intercept Ctrl+V in `NoteRichEditBox_KeyDown`, check for a bitmap on the clipboard, re-encode it as PNG via WinRT `BitmapDecoder`/`BitmapEncoder`, save to the blobs directory, and insert a `![](amadeuz://blob/{id})` Markdown reference at the cursor — which is the same format all other clients use. `e.Handled = true` prevents the default OLE paste from running. |
| Windows client rewritten in WPF, dropping WinUI 3 (March 2026) | The WinUI 3 / WinAppSDK client accumulated a set of packaging and build problems that made it fragile and hard to deploy: `WindowsAppSDKSelfContained=true` bundled native DLLs incompatible with Windows Insider Preview; `dotnet publish` failed on PRI generation, requiring MSBuild from Visual Studio; bootstrap init had to be forced on explicitly or the app crashed silently before XAML loaded; and target machines needed the Windows App Runtime installed separately. WPF with .NET 9 eliminates all of these: `dotnet build` / `dotnet run` work cleanly, self-contained publish is a single `dotnet publish` command, and no runtime install is required. The old WinUI 3 code is preserved in `notes/windows/`; the active client is `notes/windows-wpf/`. |
| ModernWpfUI over Wpf.Ui (lepoco) for WPF Fluent styling (March 2026) | The natural choice for WPF Fluent styling is `Wpf.Ui` by lepoco. However, the NuGet package ID `WPF.UI` is squatted by an unrelated Chinese package (`WPF.UI 3.1.0`, net40 only) — lepoco's package is effectively unreachable from a standard `dotnet add package` invocation. ModernWpfUI (0.9.6) provides the same set of primitives needed (`ui:WindowHelper.UseModernWindowStyle`, `AccentButtonStyle`, `TextBlockButtonStyle`, `ui:ControlHelper.PlaceholderText`, system theme watching) and installs cleanly. |
| DPAPI (`ProtectedData`) for Windows JWT storage in WPF (March 2026) | The WinUI 3 client used `Windows.Security.Credentials.PasswordVault` (Windows Credential Manager) via WinRT. Plain WPF has no access to WinRT APIs without additional interop machinery. DPAPI (`System.Security.Cryptography.ProtectedData`) is available in the .NET BCL, encrypts with the current user's Windows login credentials, and is equally secure for the single-user use case. Token is stored as an encrypted binary at `%APPDATA%\amadeuz\token.dat`. |
| CommunityToolkit.Mvvm for WPF MVVM base (March 2026) | `ObservableObject` base class and `[ObservableProperty]` source generator from CommunityToolkit.Mvvm (8.4.0) reduce boilerplate significantly — no manual `INotifyPropertyChanged` implementations needed. This is the same toolkit used on Android (Jetpack) and aligns with the modern .NET MVVM direction. Thread marshaling uses `Dispatcher.BeginInvoke` (the WPF equivalent of WinUI's `_dispatcher.TryEnqueue`). |
| Flatpak for Linux distribution (March 2026) | Flatpak gives the Linux client a clean install/uninstall story and sandboxing without touching the system package manager. It integrates with GNOME Shell (app grid, icon, launch) and is the standard distribution mechanism for GNOME apps. The alternative (RPM/DEB, or manual install) requires root and lacks the isolation story. Flatpak's `--user` install keeps it fully separate from the dev environment. |
| `cargo vendor` over `flatpak-cargo-generator` for offline Rust builds (March 2026) | The `flatpak-cargo-generator.py` tool creates a cargo registry cache (`.crate` files) but does not reproduce the registry index, so cargo in offline mode cannot resolve packages — it knows the crate files exist but cannot look them up. `cargo vendor` takes a different approach: it creates a self-contained `vendor/` directory with all source trees and a `.cargo/config.toml` that redirects `crates-io` to `directory = "vendor"`. Cargo needs no network and no index at all. The `vendor/` directory is created on the host before the sandbox starts (where network is available), then included via the `type: dir` Flatpak source. |
| Windows client: Electron rewrite, abandoning WPF (March 2026) | The Windows client has now gone through two complete rewrites — WinUI 3 (abandoned: packaging failures, DLL incompatibilities, silent bootstrap crashes, MSBuild-only publish) and WPF/ModernWpfUI (abandoned: repeated implementation failures during feature development). After multiple failed attempts at a stable native Windows client, the pragmatic decision is to use Electron for Windows only. The "native, always" principle has been relaxed for Windows because repeated native attempts produced no working result. The other platforms (macOS, iOS, Linux, Android) remain native — they are working well and will not change. The Electron client is Windows-dedicated; no other platform will use it. |
