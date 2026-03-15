# Amadeuz — Progress Tracker

> **How to use this file in a new AI session**
  >
> Open this file at the start of the session and say:
> "Read PROGRESS.md and VISION.md. I'm working on [platform]. Feature X is done on [other platform]
> — implement the same thing here."
  >
> The feature matrix shows what is implemented where. Each platform section has the full
> technical context (dependencies, build steps, file structure) needed to continue work.

---

## Feature matrix

> Android column: code is written and reviewed but not yet run on a device (blocked by
> slow dev machine). ⏳ = implemented, pending first real run.

| Feature | Server | macOS | Windows | Linux | iOS | Android |
|---------|:------:|:-----:|:-------:|:-----:|:---:|:-------:|
| Single note editor | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Offline-first local storage | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| WebSocket connection to server | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Auto-reconnect (3 s) | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| 500 ms debounce save | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Timestamp merge (last-write-wins) | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Push local-ahead note on connect | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Connection status indicator | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Configurable server address | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Persistent server address | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Server persistence (SQLite) | ✅ | — | — | — | — | — |
| Multiple notes | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Folders (create / rename / delete) | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Cascade delete (folder → notes) | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| "All Notes" view | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Drawer/sidebar navigation | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Per-note last-write-wins merge | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Create / delete notes | ✅ | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Note list with title, date, preview | — | ✅ | ❌ | ✅ | ✅ | ⏳ |
| Unfoldered notes (folder optional) | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Create note from "All Notes" view | — | ✅ | ❌ | ✅ | ✅ | ❌ |
| Move note between folders | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Folder label in note list row | — | ✅ | ❌ | ✅ | ✅ | ❌ |
| Search / filter notes | — | ✅ | ❌ | ✅ | ✅ | ❌ |
| Inline images in notes | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Offline blob queue (insert images offline) | — | ✅ | ❌ | ❌ | ✅ | ❌ |
| Markdown rich text (headers, bullets, checkboxes) | — | ✅ | ❌ | ✅ | ✅ | ❌ |
| User accounts / JWT auth | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Register / Login / Recover | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Recovery codes | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| JWT in platform credential store | — | ✅ | ❌ | ✅ | ✅ | ❌ |
| REST CRUD API | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Per-note WebSocket (live sync) | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Sign Out | — | ✅ | ❌ | ✅ | ✅ | ❌ |
| Trash (soft delete / restore / delete permanently) | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Offline mode (no account, persisted choice) | — | ❌ | ❌ | ✅ | ❌ | ❌ |
| Welcome / onboarding screen | — | ❌ | ❌ | ✅ | ❌ | ❌ |
| Image paste from clipboard (screenshots, web) | — | ❌ | ❌ | ✅ | ✅ | ❌ |
| **End-to-end encryption** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Note sharing between users** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## Server (Go)

**Status:** Phase 2a complete + trash — user accounts, JWT auth, REST API, SQLite persistence, per-note WebSocket, soft-delete (trash/restore).
**Location:** `server/`
**Run:** `cd server && go mod tidy && go run .`
**Port:** `8080` on all interfaces (`0.0.0.0`)
**Tests:** `cd server && go test ./...` (25 integration tests, all passing)

### What it does

- User registration, login, and password recovery (recovery codes — no email required)
- Admin CLI: `amadeuz-server reset-password <email> <new-password>`
- JWT authentication (HS256, 30-day expiry); JWT secret auto-generated and persisted in DB
- Full folder and note CRUD via REST
- Soft-delete (trash): `PATCH /notes/:id/trash` sets `deleted_at`; `PATCH /notes/:id/restore` clears it. `GET /notes` always returns all notes including soft-deleted; clients filter by `deleted_at`.
- Per-note WebSocket for live typing sync (`GET /notes/:id/ws?token=<jwt>`)
- Blob store: `PUT /blobs/:id` (authenticated, idempotent), `GET /blobs/:id` (unauthenticated)
- SQLite via `modernc.org/sqlite` (pure Go, no CGO — cross-compiles to ARM for Raspberry Pi)
- `PRAGMA foreign_keys = ON` + `PRAGMA journal_mode = WAL`
- `ON DELETE CASCADE` for notes when their folder is deleted
- Idempotent `ALTER TABLE` migration via `pragma_table_info` — safe to re-run on existing databases

### REST API

```
POST /auth/register           body: { email, password }                     → { token, recovery_code }
POST /auth/login              body: { email, password }                     → { token }
POST /auth/recover            body: { email, recovery_code, new_password }  → { token, recovery_code }

GET    /folders               → { folders: [...] }
POST   /folders               body: { id?, name, created_at? }              → { folder }
PATCH  /folders/:id           body: { name }                                → { folder }
DELETE /folders/:id                                                         → 204

GET    /notes                 → { notes: [...] }  (includes content; deleted_at non-null = in trash)
POST   /notes                 body: { id?, folder_id?, title, content, updated_at, created_at? } → { note }
PATCH  /notes/:id             body: { title, content, updated_at }          → { note } or 409 if stale
PATCH  /notes/:id/move        body: { folder_id }                           → { note }
PATCH  /notes/:id/trash                                                     → 204 (sets deleted_at = now)
PATCH  /notes/:id/restore                                                   → 204 (clears deleted_at)
DELETE /notes/:id                                                           → 204

PUT    /blobs/:id             (authenticated)  → 201 created / 200 already exists
GET    /blobs/:id             (unauthenticated) → blob data
```

JWT is accepted as `Authorization: Bearer <token>` header, or `?token=<jwt>` query param (for WebSocket upgrade).

### WebSocket wire protocol (per note)

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

Server applies last-write-wins: only accepts and rebroadcasts if `updated_at` is strictly greater than stored.
Ping/pong keepalive: 30 s interval, 60 s read deadline. Zombie connections are cleaned up automatically.

### Internal packages

```
server/
├── main.go                  ← thin wiring, graceful shutdown, reset-password subcommand
├── server_test.go           ← 25 integration tests
└── internal/
    ├── db/                  ← SQLite open, migrations, JWT secret, NewID
    ├── auth/                ← register, login, recover handlers + JWT middleware
    ├── folders/             ← folder CRUD handlers
    ├── notes/               ← note CRUD handlers + per-note WebSocket hub
    └── blobs/               ← blob upload/download handlers
```

### Deployment (Raspberry Pi)

- Build: `GOOS=linux GOARCH=arm64 go build -o amadeuz-server .`
- Copy: `scp amadeuz-server pi@<PI_IP>:/opt/amadeuz/amadeuz-server`
- Managed by systemd: `sudo systemctl restart amadeuz`
- DB path override: `AMADEUZ_DB=/data/amadeuz.db ./amadeuz-server`
- Logs: `sudo journalctl -u amadeuz -f`

### Dependencies

- `github.com/go-chi/chi/v5` — router
- `github.com/golang-jwt/jwt/v5` — JWT sign/verify
- `golang.org/x/crypto/bcrypt` — password hashing
- `modernc.org/sqlite` — pure Go SQLite
- `github.com/gorilla/websocket` — WebSocket

---

## macOS (Swift + SwiftUI)

**Status:** Phase 2c complete — Apple Notes-style layout, trash, date grouping, note counts, full feature parity with Linux client.
**Location:** `notes/mac/`
**Run:** `cd notes/mac && swift run`
**Requires:** Xcode command-line tools + accepted license (`sudo xcodebuild -license`)

### What it does

- Login / Register / Recover screen with segmented control (shown before main UI)
- Recovery code displayed in a sheet after register or recover — shown once, copy button provided
- **Apple Notes-style three-column `NavigationSplitView`**: folder sidebar | note list with date grouping | note editor
- **Folder sidebar**: "All Notes" (with total note count badge) → user folders (with per-folder count badges) → "Recently Deleted" (trash, with count badge). "New Folder" button pinned at bottom
- **Note list with date grouping**: sections "Today", "Previous 7 Days", "Previous 30 Days", "Older" — auto-computed from `updatedAt` timestamps
- **Note rows**: bold title + (date + content preview on one line) + folder label — matches Apple Notes visual style
- **Note editor**: date stamp centered at top (e.g. "12 March 2026 at 17:20"), then title field, then body
- **Trash (soft delete)**: right-click note → "Move to Trash". Trash view shows Restore / Delete Permanently context menu. "Empty Trash" toolbar button when trash has notes
- Sign Out accessible from app menu (macOS menu bar → Notes → Sign Out)
- Cmd+N creates a new note in the current folder
- All CRUD (folders, notes) via REST; 500 ms debounced note content changes via REST PATCH
- Per-note WebSocket opens when a note is selected — receives live updates from other clients
- Full sync on login/reconnect: merges server state with local by last-write-wins, pushes any offline-created or locally-newer items
- Offline-first: local state updated immediately; REST calls fire in background; full sync on reconnect catches up
- JWT stored in macOS Keychain; survives app restart without re-login
- Inline images (drag & drop / paste), Markdown styling (headers, bullets, checkboxes), offline blob queue

### Local storage

`~/Library/Application Support/amadeuz/data.json`
```json
{ "folders": [...], "notes": [...] }
```

Note model includes `deleted_at` (Int64, optional) for trash state.

### Settings storage

`UserDefaults` — key `"serverAddress"`. Default: `http://localhost:8080`

### Key files

| File | Role |
|------|------|
| `NoteApp.swift` | `@main` entry; `@StateObject vm`; `.commands` adds Sign Out + Cmd+N |
| `AuthView.swift` | Login / Register / Recover UI; `RecoveryCodeView` sheet |
| `KeychainStore.swift` | Save, load, delete JWT from macOS Keychain |
| `APIClient.swift` | All REST calls + WebSocket URL builder; `trashNote`, `restoreNote` |
| `Models.swift` | `Folder`, `Note` (with `deletedAt`), `AuthResponse`, `NoteWsMsg` |
| `ContentView.swift` | Auth gate; Apple Notes 3-column layout; `FolderSidebar`, `NoteList`, `NoteEditor`, `SettingsView` |
| `NoteViewModel.swift` | `@MainActor NotesViewModel` — auth, `fullSync()`, CRUD, trash/restore, date sections |
| `SyncService.swift` | `NoteSync` — per-note WebSocket, receive-only (init + update), auto-reconnect |
| `BlobStore.swift` | Local blob cache, pending upload queue, authenticated `PUT /blobs/:id` |
| `LocalStore.swift` | Read/write `data.json` (folders + notes) in Application Support |
| `MarkdownEditor.swift` | `NSTextView`-based rich editor: inline images (paste/drop), Markdown styling |

### Virtual folder sentinels

| Sentinel | Meaning |
|----------|---------|
| `"__all__"` | All Notes — shows all active (non-trashed) notes |
| `"__trash__"` | Recently Deleted — shows notes where `deletedAt != nil` |

### Known quirks

- SPM executables don't activate as foreground apps by default. Fixed with `AppDelegate`:
  `NSApp.setActivationPolicy(.regular)` + `NSApp.activate(ignoringOtherApps: true)`
- Recovery code sheet is attached to `ContentView`'s root view (not `AuthView`) so it survives the auth state transition that removes `AuthView` from the hierarchy
- Trash state is persisted locally in `data.json` via `deleted_at` field; syncs with server via `PATCH /notes/:id/trash` and `PATCH /notes/:id/restore`

---

## Windows (Electron)

**Status:** Not started — fresh Electron rewrite. All features pending.
**Location:** `notes/windows-electron/` (to be created)
**Stack:** Electron + HTML/CSS/JS (Node.js main process, renderer process)

> **Note:** Previous WPF client (`notes/windows-wpf/`) is preserved as legacy reference.
> Previous WinUI 3 client (`notes/windows/`) is also preserved.

### Why the switch from WinUI 3 to WPF

WinUI 3 / WinAppSDK accumulated several packaging and build issues:
- `WindowsAppSDKSelfContained=true` bundled native DLLs incompatible with Windows Insider Preview
- `dotnet publish` fails on WinUI 3 PRI generation — MSBuild from VS required
- Bootstrap init (`WindowsAppSdkBootstrapInitialize=true`) crashes silently if omitted
- Target machine requires Windows App Runtime 1.8 installed separately

WPF avoids all of these. `dotnet build` and `dotnet run` work cleanly. Self-contained
publish is `dotnet publish -r win-x64 --self-contained`. No runtime install required.

The preferred styling library `Wpf.Ui` (by lepoco) is effectively unavailable from nuget.org —
the `WPF.UI` package ID is squatted by an unrelated Chinese package (`WPF.UI 3.1.0`, net40 only).
**ModernWpfUI** (0.9.6) was used instead — it provides Fluent Design / Windows 10/11 styling
without external DLL complications.

### Stack

| Layer | Choice |
|-------|--------|
| Language | C# |
| Framework | WPF (.NET 9) |
| Styling | ModernWpfUI 0.9.6 |
| MVVM base | CommunityToolkit.Mvvm 8.4.0 (`ObservableObject`, `[ObservableProperty]`) |
| Thread marshaling | `Dispatcher.BeginInvoke` |
| Credential store | DPAPI (`ProtectedData`) — `%APPDATA%\amadeuz\token.dat` |

### What it does

- **Auth**: Login / Register / Recover overlay (full-screen card) with server URL, email, password, recovery-code, and new-password fields. Mode tabs switch between Log In / Register / Recover. Recovery code shown in a `MessageBox` after registration or recovery. JWT stored via DPAPI at `%APPDATA%\amadeuz\token.dat`.
- **Three-column layout**: folder sidebar | note list | note editor
- **Folder sidebar**: "All Notes" sentinel → user folders → "Wastebasket" sentinel (pinned at bottom). Create/rename/delete folders via right-click context menu (blocked on sentinels). "New Folder" button at bottom.
- **Note list**: sorted by `updatedAt` descending; title, date, content preview. Search bar filters in real time. Right-click → Move to Trash (normal view) or Restore / Delete Permanently (wastebasket view). Move to Folder submenu lists all user folders.
- **Single-body `RichTextBox` editor**: first line is the title; remaining lines are the body. Disabled until a note is selected.
- **Markdown formatting**: visual formatting via `TextRange` / `Run` inline properties — `# / ## / ###` headings, bullets, checkboxes, `**bold**`, `_italic_`, `~~strikethrough~~`. Format pass debounced at 120 ms.
- **Inline images**: Ctrl+V with an image on the clipboard saves to `%APPDATA%\amadeuz\blobs\{id}.png`, queues upload via `PUT /blobs/{id}`, inserts `![](amadeuz://blob/{id})` at the cursor.
- **Folder label in note list rows**: each row shows folder icon + folder name (or "—" for unfoldered notes).
- 500 ms debounce on any change → save locally + REST PATCH to server.
- **Full offline-first**: loads `data.json` on startup; works without server. On connect: full sync — REST GET /folders + GET /notes, merges by last-write-wins, pushes any offline-created or locally-newer notes/folders.
- **Per-note WebSocket**: opens when a note is selected (`GET /notes/:id/ws?token=<jwt>`); receives live title+content updates from other clients; auto-reconnects every 3 s.
- **Trash (soft delete)**: "Wastebasket" virtual folder shows notes with `deleted_at` set. `PATCH /notes/:id/trash` / `PATCH /notes/:id/restore` / `DELETE /notes/:id`.
- **Move note**: right-click → Move to Folder submenu; `PATCH /notes/:id/move`.
- **Search**: search box → `Vm.SetSearchQuery()` → filtered `ObservableCollection` diff.
- **Sign Out**: clears state, deletes DPAPI token file, shows auth overlay.
- Green/red status dot; ModernWpfUI system theme watching for dark/light mode.

### Virtual folder sentinels

| Sentinel | Meaning |
|----------|---------|
| `"__all__"` | All Notes — shows all active (non-trashed) notes |
| `"__wastebasket__"` | Wastebasket — shows notes where `deleted_at != null` |

### Local storage
`%APPDATA%\amadeuz\data.json`
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage
`%APPDATA%\amadeuz\settings.json` — JSON `{ "serverAddress": "http://..." }`
Default server: `http://localhost:8080`

### Credential storage
`%APPDATA%\amadeuz\token.dat` — DPAPI-encrypted JWT (no WinRT available in plain WPF).
`ProtectedData.Protect` / `ProtectedData.Unprotect` with `DataProtectionScope.CurrentUser`.

### Key files
| File | Role |
|------|------|
| `MainWindow.xaml` | Two-layer Grid: notes UI + full-screen auth overlay; DataTemplates for folder/note lists; SearchBox; auth card with mode tabs |
| `MainWindow.xaml.cs` | Auth overlay show/hide; mode tab switching; recovery code display; context-sensitive right-click menus; search wiring |
| `NotesViewModel.cs` | Auth, `FullSync`, CRUD, trash/restore/move, search filter, debounce, per-note WS lifecycle, offline-first merge. `ObservableObject` base via CommunityToolkit.Mvvm; `[ObservableProperty]` for bound properties; `Dispatcher.BeginInvoke` for thread marshaling. |
| `ApiClient.cs` | All REST calls (auth, folders, notes); DPAPI token store; `WsUrlForNote`; `NormaliseUrl` |
| `Models.cs` | `Folder`, `Note` (with `DeletedAt`, `INotifyPropertyChanged`), `FolderItem` (with `IsWastebasket`/`IsSpecial`), `NoteWsMessage`, `AuthResponse` |
| `LocalStore.cs` | Read/write `data.json` (folders + notes) via `System.Text.Json` |
| `SyncService.cs` | Per-note `ClientWebSocket` wrapper, `NoteWsMessage` delivery, auto-reconnect every 3 s |

### ModernWpfUI notes
- `ui:WindowHelper.UseModernWindowStyle="True"` on the Window — enables Fluent chrome
- `AccentButtonStyle` and `TextBlockButtonStyle` available as static resources
- `ui:ControlHelper.PlaceholderText` attached property for placeholder text on inputs
- System theme watching included — app follows Windows dark/light mode automatically
- No Mica backdrop (WPF does not support DWM backdrop APIs directly; requires P/Invoke)

---

## Linux (Rust + GTK4 + Libadwaita)

**Status:** Phase 2 complete + UX polish — auth, offline mode, trash, inline images (file + clipboard), welcome screen, inline folder creation, 3-line note rows, live markdown.
**Location:** `notes/linux/`

> **Context for a new session:** The C++ GTK4 client was brought to full Phase 2a feature parity
> (all features ✅ in the matrix above). That code is preserved in `legacy-code/linux/` as
> reference for the feature set and logic. The fresh Rust client is now at Phase 2 parity.

### Stack

| Layer | Choice |
|-------|--------|
| Language | Rust (gtk4-rs 0.10, glib 0.21) |
| Toolkit | GTK4 4.20 + Libadwaita 1.8 |
| UI definition | Blueprint 0.18 (`.blp` files → compiled to `.ui` by Meson) |
| Build system | Meson 1.8 + Cargo |
| Config | GSettings (`com.amadeuz.Notes.gschema.xml`) |
| Credentials | `secret-service` crate v3 (D-Bus Secret Service / GNOME Keyring) |
| Async | Tokio (multi-thread) + tokio-tungstenite 0.26 + reqwest 0.12 |
| Markdown | `pulldown-cmark` 0.12 (pure Rust) — TextTag formatting + image detection |
| Channel | `async_channel` 2 (tokio → GTK main thread; glib::Sender removed in glib 0.21) |
| Local data | `~/.local/share/amadeuz/data.json` (same format as all other clients) |

### Build

**Dev build (local, not sandboxed):**
```bash
# Install dependencies (Fedora)
sudo dnf install meson cargo rust libadwaita-devel libsecret-devel blueprint-compiler

cd notes/linux
meson setup build --prefix=$HOME/.local -Dprofile=development
ninja -C build
meson install -C build
glib-compile-schemas ~/.local/share/glib-2.0/schemas/

# Run
GSETTINGS_SCHEMA_DIR=~/.local/share/glib-2.0/schemas ~/.local/bin/amadeuz-notes
```

**Flatpak build (sandboxed, appears in GNOME app launcher):**
```bash
cd notes/linux
./build-flatpak.sh   # installs prerequisites, vendors deps, builds, installs as user Flatpak

flatpak run com.amadeuz.Notes
```

The script handles everything on first run: installs `flatpak-builder` via dnf, installs the GNOME SDK + Rust extension from Flathub, runs `cargo vendor vendor/` to pre-fetch all crates (needed because the sandbox has no network), and installs the app. Re-run after code changes to rebuild.

If `Cargo.lock` changes, delete `vendor/` and `.cargo/` before re-running so they are regenerated.

### Source layout

```
notes/linux/
├── meson.build              ← project(), dependencies, subdir() calls
├── meson_options.txt        ← profile=default|development
├── Cargo.toml               ← Rust dependencies
├── Cargo.lock               ← committed (required for reproducible Flatpak builds)
├── com.amadeuz.Notes.yaml   ← Flatpak manifest
├── build-flatpak.sh         ← one-shot: vendor deps, build, install as user Flatpak
├── data/
│   ├── meson.build          ← Blueprint compile, compile_resources, gschema/desktop/metainfo install
│   ├── com.amadeuz.Notes.gschema.xml    ← GSettings: server-url key
│   ├── com.amadeuz.Notes.gresource.xml  ← GResource manifest (window.ui, auth.ui, note_row.ui)
│   ├── com.amadeuz.Notes.desktop        ← desktop entry (makes app appear in GNOME Shell)
│   ├── com.amadeuz.Notes.metainfo.xml   ← AppStream metadata
│   ├── icons/hicolor/256x256/apps/
│   │   └── com.amadeuz.Notes.png        ← app icon (sourced from icons/simple_notebook.png)
│   └── ui/
│       ├── window.blp       ← Blueprint: 3-column layout (folders | note list | editor)
│       ├── auth.blp         ← Blueprint: login / register / recover UI
│       └── note_row.blp     ← Blueprint: note list row (title + date + preview)
└── src/
    ├── meson.build          ← cargo custom_target with MESON_* env injection
    ├── main.rs              ← entry: tokio runtime, load gresource, run GTK
    ├── config.rs            ← MESON_* compile-time statics (APP_ID, DATADIR, …)
    ├── app.rs               ← AmzApplication: reads GSettings, loads keyring, creates manager
    ├── manager.rs           ← NotesManager: event loop, auth, CRUD, debounce, WS lifecycle
    ├── model/
    │   ├── mod.rs
    │   ├── note.rs          ← AmzNote GObject (id, title, content, folder_id, timestamps)
    │   └── folder.rs        ← AmzFolder GObject (id, name, created_at)
    ├── ui/
    │   ├── mod.rs
    │   ├── auth.rs          ← AmzAuthView (AdwBin, CompositeTemplate)
    │   ├── md_formatter.rs  ← Markdown TextTag formatting, smart-Enter list continuation, image embedding
    │   ├── note_row.rs      ← AmzNoteRow (GtkBox, CompositeTemplate)
    │   └── window.rs        ← AmzWindow (AdwApplicationWindow, CompositeTemplate)
    └── backend/
        ├── mod.rs
        ├── api_client.rs    ← REST API client (reqwest, all endpoints)
        ├── local_store.rs   ← load/save data.json via glib::user_data_dir()
        ├── keyring.rs       ← JWT store/load/delete via secret-service crate
        └── sync_worker.rs   ← NoteSync RAII handle: WS per note, auto-reconnect, cancel via oneshot
```

### What it does

- **Welcome / onboarding screen**: `Adw.StatusPage` with two `Adw.ActionRow`s (Use Offline / Connect to Server) and a Quit button. Shown on first launch when no token or offline-mode flag is found.
- **Offline mode**: User chooses "Use Offline" on the welcome screen. `offline_mode = true` is persisted in `data.json`. The app works fully — create/edit/delete notes and folders — with no server. The status bar shows "Offline Mode". Menu shows "Return to Start" instead of "Sign Out".
- **Auth flow**: Login, Register, and Recover pages in a `Gtk.Stack`. Navigation between pages uses inline links (`flat` buttons) rather than a tab bar — discoverable without taking up vertical space. Navigating between auth pages re-enables all buttons (an in-flight request on one page cannot leave another page's button greyed out). JWT stored in GNOME Keyring via `libsecret`.
- **Trash / wastebasket**: Notes can be moved to trash (soft delete). Trash is shown as a pinned row at the bottom of the folder sidebar. In the trash, context menu shows "Restore" and "Delete Permanently". Permanent delete shows a destructive `Adw.AlertDialog` confirmation. Synced with server via `PATCH /notes/:id/trash` and `PATCH /notes/:id/restore`. Client maps `deleted_at` non-null ↔ `folder_id = "__wastebasket__"` sentinel.
- **Inline folder creation**: Clicking `+` in the folder header reveals a `Gtk.Revealer` with a `Gtk.Entry` inline. Enter confirms, Escape cancels. No modal dialog.
- **3-line note rows**: Title / content preview / [folder name (left) · relative date (right)]. `trim_start()` on preview strips blank lines between title and first content line.
- **Image paste from clipboard**: `gdk::Texture` branch in the paste handler saves screenshots and web-copied images as PNGs to `~/.local/share/amadeuz/images/{note_id}/`. Previously only `gdk::FileList` (drag from file manager) was handled.
- **Dynamic menu**: `gio::Menu` is rebuilt in `refresh_ui`. Authenticated state shows "Server Settings" + "Sign Out"; offline state shows only "Return to Start".
- **Auto-select first note** on app start and after creating a new note (via `glib::idle_add_local_once` deferred row selection).

### Key implementation notes

- **glib 0.21**: `glib::Sender/Receiver` removed. Use `async_channel::bounded()` + `glib::MainContext::default().spawn_local()` to receive events on the GTK main thread.
- **Tokio handle**: `TOKIO_HANDLE: OnceLock<tokio::runtime::Handle>` in `main.rs`; accessed globally via `crate::spawn()`.
- **Blueprint StackPage names**: Must use explicit `Gtk.StackPage { name: "auth"; child: ... }` — setting `name:` on the widget itself only sets `GtkWidget.name` (CSS), not the page name. This is the source of "Child name 'X' not found in GtkStack" runtime warnings.
- **`glib::wrapper!` @implements**: ApplicationWindow subclasses need the full set: `gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager` plus `gio::ActionGroup, gio::ActionMap`.
- **`gio::Settings::with_path`**: In gio 0.21, `new_with_path` was renamed to `with_path` (no longer returns a Result — panics if schema not found). Guard with `gio::SettingsSchemaSource::default().and_then(|src| src.lookup(...))`.
- **WS callbacks must be `Sync`**: `NoteSync::connect` closures (`on_msg`, `on_status`) are called from inside `tokio::spawn`, which requires `Send + Sync`.
- **`ObjectImpl::constructed()` for internal widget wiring**: Use this override (not `instance_init`) to wire signals between template children — template children are bound by the time `constructed()` runs.
- **`gdk::Clipboard::read_texture_async`** returns `Result<Option<Texture>, glib::Error>` — the `Option` is `None` if the clipboard had no image data, not an error.
- Must build via Meson — `cargo build` alone won't inject MESON_* env vars into `config.rs`.

### Primary reference

**Fragments** (`external-projects/Fragments-main/`) — mature Rust + GTK4 + Libadwaita + Meson app.

---

## iOS (Swift + SwiftUI)

**Status:** Full feature parity with macOS — rebuilt from scratch against the current server and mac client.
**Location:** `notes/ios/`
**Build:** `open notes/ios/Notes.xcodeproj` in Xcode → select device or simulator → Run
**Requires:** Xcode + Apple Developer account (free tier works for personal device)
**Old iOS code:** preserved in `legacy-code/ios/`

### What it does

- Login / Register / Recover screen (segmented control), same as macOS
- Recovery code displayed in a sheet after register or recover — copy button uses `UIPasteboard`
- **`NavigationSplitView` three-column layout**: folder sidebar | note list with date grouping | note editor
- **Folder sidebar**: "All Notes", user folders, "Recently Deleted" — all with count badges. "New Folder" button pinned at bottom. Settings (⚙) icon in top-right of sidebar
- **Note list**: sections "Today", "Previous 7 Days", "Previous 30 Days", "Older" — same date grouping as macOS
- **Note rows**: bold title + (date + content preview) + folder label — same layout as macOS
- **Note editor**: date stamp centered at top, UITextView-based markdown editor
- **Trash**: right-click/long-press → "Move to Trash"; trash view shows Restore / Delete Permanently; "Empty Trash" toolbar button
- **Inline images**: paste from `UIPasteboard` (e.g. screenshots) + PhotosPicker toolbar button (`photo.badge.plus`)
- **Markdown styling**: headers, bullet lists, checkboxes — same visual rules as macOS; smart Enter list continuation
- **Offline blob queue**: images saved locally first; uploaded to server on reconnect
- All CRUD via REST; 500 ms debounced note changes via REST PATCH
- Per-note WebSocket for live sync; auto-reconnect every 3 s
- Full offline-first sync on login/reconnect (same merge logic as macOS)
- JWT stored in iOS Keychain via Security framework

### Local storage

`<App>/Library/Application Support/amadeuz/data.json`
```json
{ "folders": [...], "notes": [...] }
```

Blob cache: `<App>/Library/Application Support/amadeuz/blobs/`
Pending upload queue: `<App>/Library/Application Support/amadeuz/pending_blobs.json`

### Settings storage

`UserDefaults` — key `"serverAddress"`. Default: `http://localhost:8080`
Settings sheet presented as a `NavigationStack`-wrapped `Form`.

### Key files

| File | Role |
|------|------|
| `NoteApp.swift` | `@main` entry — no AppDelegate needed on iOS |
| `AuthView.swift` | Login / Register / Recover UI; `RecoveryCodeView` (uses `UIPasteboard`) |
| `KeychainStore.swift` | Save, load, delete JWT from iOS Keychain (identical to macOS) |
| `APIClient.swift` | All REST calls + WebSocket URL builder (identical to macOS) |
| `Models.swift` | `Folder`, `Note`, `AuthResponse`, `NoteWsMsg` (identical to macOS) |
| `ContentView.swift` | Auth gate; 3-column `NavigationSplitView`; `FolderSidebar`, `NoteList`, `NoteEditor`, `SettingsView` |
| `NoteViewModel.swift` | `@MainActor NotesViewModel` — auth, `fullSync()`, CRUD, trash/restore, date sections (identical to macOS) |
| `SyncService.swift` | `NoteSync` — per-note WebSocket, auto-reconnect (identical to macOS) |
| `BlobStore.swift` | Local blob cache, pending upload queue, `PUT /blobs/:id` (identical to macOS) |
| `LocalStore.swift` | Read/write `data.json` in Application Support (identical to macOS) |
| `MarkdownEditor.swift` | `UITextView`-based rich editor: inline images (paste + PhotosPicker), Markdown styling |

### Platform differences vs macOS

| macOS | iOS |
|-------|-----|
| `NSTextView` + `NSScrollView` | `UITextView` (scroll built-in) |
| `NSImage` | `UIImage` |
| `NSPasteboard` | `UIPasteboard` |
| Drag & drop for images | PhotosPicker toolbar button + paste |
| Settings in editor toolbar | Settings in sidebar toolbar (⚙) |
| `.navigationSubtitle(...)` | Dropped (not available on iOS) |
| `AppDelegate` activation hack | Not needed |
| Cmd+N menu command | Not applicable |

### Code shared verbatim with macOS

`Models.swift`, `APIClient.swift`, `KeychainStore.swift`, `SyncService.swift`, `LocalStore.swift`, `BlobStore.swift`, `NoteViewModel.swift` — all identical.

### Known quirks

- `NavigationSplitView` on iPhone collapses to a stack — three-column becomes drill-down. Works as intended on iPad and in landscape on large iPhones.
- Image insertion via PhotosPicker passes `Data` through a `@Binding var pendingImageData` on `MarkdownEditor` to avoid UIKit/SwiftUI bridging complexity.

---

## Android (Kotlin + Jetpack Compose)

**Status:** Code complete, not yet run on a device. Pending test on a faster machine.
**Location:** `notes/android/`
**Build:** Open in Android Studio, sync Gradle, run on emulator or physical device

### What it does (by design — pending first run)
- Navigation drawer for folder list (replaces three-column layout — mobile-appropriate)
- Note list as main screen; tapping a note opens a full-screen editor
- FAB to create a new note (only visible when a real folder is selected)
- Full offline-first: loads `data.json` on startup, works without server
- Debounce: 500ms after last change to title or content → save locally + push to server
- Per-note last-write-wins merge on reconnect (push local if ahead)
- Auto-reconnect every 3 seconds; green/red dot in top bar
- Settings dialog for configurable server URL (stored in SharedPreferences)
- "All Notes" virtual view across all folders

### Local storage
`<App>/files/data.json` (internal storage, via `context.filesDir`)
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage
`SharedPreferences` — key `"serverAddress"`
Default server: `ws://localhost:8080/ws`

### Key files
| File | Role |
|------|------|
| `Models.kt` | `Folder`, `Note`, `WSMsg` data classes |
| `LocalStore.kt` | Read/write `data.json` via Gson |
| `SyncService.kt` | OkHttp WebSocket, Handler-based 3s reconnect |
| `NoteViewModel.kt` | `AndroidViewModel` — StateFlow state, debounce, sync, CRUD |
| `NoteScreen.kt` | All Compose UI: drawer, note list, editor, settings dialog |
| `MainActivity.kt` | Wires ViewModel into Compose via `by viewModels()` |

### Dependencies added (beyond scaffolded defaults)
| Library | Version | Used for |
|---------|---------|----------|
| `com.squareup.okhttp3:okhttp` | 4.12.0 | WebSocket client |
| `com.google.code.gson:gson` | 2.10.1 | JSON serialization |
| `androidx.compose.material:material-icons-core` | BOM-managed | UI icons |

### Architecture notes
- `NoteViewModel` is an `AndroidViewModel` — needs `Application` context for `LocalStore` and `SharedPreferences`
- All StateFlows are `MutableStateFlow` internally, exposed as `StateFlow` (read-only)
- Debounce uses `viewModelScope.launch { delay(500) }` with `Job.cancel()` on each keystroke
- WebSocket callbacks (OkHttp threads) dispatch to main thread via `viewModelScope.launch(Dispatchers.Main)`
- Reconnect delay uses `android.os.Handler(Looper.getMainLooper()).postDelayed()` — no extra threading needed
- Gson default behaviour omits null fields from serialization — no custom serializer needed

### Build issues encountered (already fixed)
- `kotlin-android` plugin conflict: AGP 9.x already applies it internally — adding it explicitly causes `Cannot add extension 'kotlin'`. Removed from both `build.gradle.kts` files.
- `Icons.Default.Circle` and `Icons.Default.CreateNewFolder` are in `material-icons-extended`, not core. Replaced: `Circle` → `Box` with `CircleShape` background; `CreateNewFolder` → `Icons.Default.Add`.

### Known differences from other clients
- Mobile layout uses a navigation drawer instead of a persistent column sidebar
- No persistent three-column view (not appropriate for phone screens)
- Context menus accessed via `MoreVert` (⋮) icon buttons rather than right-click
