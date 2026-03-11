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
| Single note editor | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Offline-first local storage | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| WebSocket connection to server | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Auto-reconnect (3 s) | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| 500 ms debounce save | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Timestamp merge (last-write-wins) | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Push local-ahead note on connect | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Connection status indicator | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Configurable server address | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Persistent server address | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Server persistence (SQLite) | ✅ | — | — | — | — | — |
| Multiple notes | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Folders (create / rename / delete) | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Cascade delete (folder → notes) | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| "All Notes" view | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Drawer/sidebar navigation | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Per-note last-write-wins merge | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Create / delete notes | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Note list with title, date, preview | — | ✅ | ✅ | ✅ | ✅ | ⏳ |
| Unfoldered notes (folder optional) | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Create note from "All Notes" view | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| Move note between folders | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Folder label in note list row | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| Search / filter notes | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| Inline images in notes | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Offline blob queue (insert images offline) | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| Markdown rich text (headers, bullets, checkboxes) | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| User accounts / JWT auth | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Register / Login / Recover | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Recovery codes | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| JWT in platform credential store | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| REST CRUD API | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Per-note WebSocket (live sync) | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Sign Out | — | ✅ | ❌ | ❌ | ❌ | ❌ |
| **End-to-end encryption** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Note sharing between users** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## Server (Go)

**Status:** Phase 2a complete — user accounts, JWT auth, REST API, SQLite persistence, per-note WebSocket.
**Location:** `server/`
**Run:** `cd server && go mod tidy && go run .`
**Port:** `8080` on all interfaces (`0.0.0.0`)
**Tests:** `cd server && go test ./...` (25 integration tests, all passing)

### What it does

- User registration, login, and password recovery (recovery codes — no email required)
- Admin CLI: `amadeuz-server reset-password <email> <new-password>`
- JWT authentication (HS256, 30-day expiry); JWT secret auto-generated and persisted in DB
- Full folder and note CRUD via REST
- Per-note WebSocket for live typing sync (`GET /notes/:id/ws?token=<jwt>`)
- Blob store: `PUT /blobs/:id` (authenticated, idempotent), `GET /blobs/:id` (unauthenticated)
- SQLite via `modernc.org/sqlite` (pure Go, no CGO — cross-compiles to ARM for Raspberry Pi)
- `PRAGMA foreign_keys = ON` + `PRAGMA journal_mode = WAL`
- `ON DELETE CASCADE` for notes when their folder is deleted

### REST API

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

**Status:** Phase 2a complete — user accounts, JWT auth, REST sync, per-note WebSocket live sync, recovery codes.
**Location:** `notes/mac/`
**Run:** `cd notes/mac && swift run`
**Requires:** Xcode command-line tools + accepted license (`sudo xcodebuild -license`)

### What it does

- Login / Register / Recover screen with segmented control (shown before main UI)
- Recovery code displayed in a sheet after register or recover — shown once, copy button provided
- Three-column `NavigationSplitView`: folder list | note list | note editor (shown after login)
- Sign Out accessible from app menu (macOS menu bar → Notes → Sign Out); disabled when not logged in
- All CRUD (folders, notes) via REST; 500 ms debounced note content changes via REST PATCH
- Per-note WebSocket opens when a note is selected — receives live updates from other clients
- Full sync on login/reconnect: merges server state with local by last-write-wins, pushes any offline-created or locally-newer items
- Offline-first: local state updated immediately; REST calls fire in background; full sync on reconnect catches up
- JWT stored in macOS Keychain; survives app restart without re-login
- Server address format: `http://localhost:8080` (old `ws://` format auto-migrated on first launch)
- Inline images (drag & drop / paste), Markdown styling, offline blob queue — unchanged from Phase 2b

### Local storage

`~/Library/Application Support/amadeuz/data.json`
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage

`UserDefaults` — key `"serverAddress"`. Default: `http://localhost:8080`

### Key files

| File | Role |
|------|------|
| `NoteApp.swift` | `@main` entry; `@StateObject vm` owned here; `.commands` adds Sign Out to app menu |
| `AuthView.swift` | Login / Register / Recover UI; `RecoveryCodeView` sheet |
| `KeychainStore.swift` | Save, load, delete JWT from macOS Keychain |
| `APIClient.swift` | All REST calls + WebSocket URL builder; `APIError` carries server message |
| `Models.swift` | `Folder`, `Note`, `AuthResponse`, `NoteWsMsg` |
| `ContentView.swift` | Auth gate; `NavigationSplitView`; recovery code sheet at root level |
| `NoteViewModel.swift` | `@MainActor NotesViewModel` — auth state, `fullSync()`, REST CRUD, per-note WS lifecycle |
| `SyncService.swift` | `NoteSync` — per-note WebSocket, receive-only (init + update), auto-reconnect |
| `BlobStore.swift` | Local blob cache, pending upload queue, authenticated `PUT /blobs/:id` |
| `LocalStore.swift` | Read/write `data.json` (folders + notes) in Application Support |
| `MarkdownEditor.swift` | `NSTextView`-based rich editor: inline images (paste/drop), Markdown styling |

### Known quirks

- SPM executables don't activate as foreground apps by default. Fixed with `AppDelegate`:
  `NSApp.setActivationPolicy(.regular)` + `NSApp.activate(ignoringOtherApps: true)`
- Recovery code sheet is attached to `ContentView`'s root view (not `AuthView`) so it survives the auth state transition that removes `AuthView` from the hierarchy

---

## Windows (C# + WinUI 3)

**Status:** Phase 2b complete — folders + multiple notes, three-column layout
**Location:** `notes/windows/`
**Build (dev):** Open `Amadeuz.sln` in Visual Studio 2022, press F5
**Requires:** Visual Studio 2022 + Windows App SDK workload, Windows 11

### Publish (xcopy-deployable folder)

Use MSBuild from Visual Studio — **not** `dotnet publish` (it fails on WinUI 3 PRI generation):

```
"C:\Program Files\Microsoft Visual Studio\2022\Community\MSBuild\Current\Bin\MSBuild.exe" \
  notes/windows/Amadeuz/Amadeuz.csproj \
  -t:Publish \
  -p:Configuration=Release \
  -p:RuntimeIdentifier=win-x64 \
  -p:SelfContained=true \
  -p:PublishDir=notes/windows/publish
```

The output folder is self-contained for .NET (no .NET install needed on target), but requires
**Windows App Runtime 1.8** to be installed on the machine. On first run on a new machine,
the app shows a dialog offering to install it automatically.

### Key csproj settings
- `WindowsPackageType=None` — unpackaged, no MSIX, no Developer Mode needed
- `WindowsAppSdkBootstrapInitialize=true` — must be explicit; without it XAML crashes silently
- No `WindowsAppSDKSelfContained` — bundling native WinAppSDK DLLs causes crashes on Windows Insider Preview
- Custom target `CopyPriFilesToPublish` — copies `*.pri` resource files to publish output (required for XAML)

### What it does
- Three-column layout: folder sidebar | note list | note editor
- Create/rename/delete folders (context menu on right-click)
- Create/delete notes (toolbar buttons + right-click context menu)
- Note list sorted by `updatedAt` descending, with title, date, and content preview
- Full offline-first: loads `data.json` on startup, works without server
- Debounce: 500ms after last change to title or content → save locally + push to server
- Per-note last-write-wins merge on reconnect (push local if ahead)
- Auto-reconnect every 3 seconds; green/red status dot in status bar
- "All Notes" virtual folder shows all notes across all folders

### Local storage
`%APPDATA%\amadeuz\data.json`
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage
`%APPDATA%\amadeuz\settings.json` — JSON `{ "serverAddress": "ws://..." }`
Default server: `ws://localhost:8080/ws`

### Key files
| File | Role |
|------|------|
| `MainWindow.xaml` | Three-column Grid layout; DataTemplates for folder/note lists |
| `MainWindow.xaml.cs` | Event wiring; `_suppressEditorChanged` flag; dialog helpers |
| `NotesViewModel.cs` | State, debounce (`CancellationTokenSource` + `Task.Delay`), sync, CRUD |
| `Models.cs` | `Folder`, `Note`, `FolderItem`, `WsMessage` types |
| `LocalStore.cs` | Read/write `data.json` (folders + notes) via `System.Text.Json` |
| `SyncService.cs` | `ClientWebSocket` wrapper, typed `WsMessage` delivery, auto-reconnect |

### Visual notes
- Uses Mica backdrop (`SystemBackdrop = new MicaBackdrop()`) for native Windows 11 look
- Status dot is a WinUI `Ellipse` with `Fill` changed in code-behind
- All dialogs (Settings, New Folder, Rename) are `ContentDialog` built in code (no XAML)
- `FolderItem` sentinel class unifies "All Notes" + real folders into one ListView

---

## Linux (C++ + GTK4)

**Status:** Phase 2b complete — folders + multiple notes, three-column layout
**Location:** `notes/linux/`

### Build

```bash
# Install dependencies (Ubuntu / Debian) — requires GTK 4.8+
sudo apt install cmake build-essential libgtk-4-dev libsoup-3.0-dev libjson-glib-dev

# Configure + build
cd notes/linux
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build

# Run
./build/amadeuz
```

### What it does
- Three-column layout: folder sidebar | note list | note editor (via nested `GtkPaned`)
- Create folders (dialog), rename/delete folders (right-click context menu on folder row)
- Create notes (toolbar button), delete notes (toolbar button + right-click context menu)
- Note list sorted by `updated_at` descending, showing title, date, and content preview
- "All Notes" virtual view shows all notes across all folders
- Full offline-first: loads `data.json` on startup, works without server
- Debounce: 500ms after last change to title or content → save locally + push to server
- Per-note last-write-wins merge on reconnect (push local if ahead)
- Auto-reconnect every 3 seconds; green/red status dot in `GtkHeaderBar`
- Settings dialog for server URL (stored in `settings.json`)

### Local storage
`~/.local/share/amadeuz/data.json` (XDG_DATA_HOME)
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage
`~/.local/share/amadeuz/settings.json` — JSON `{ "serverAddress": "ws://..." }`
Default server: `ws://localhost:8080/ws`

### Dependencies
| Library | Used for |
|---------|----------|
| `gtk4` (≥ 4.8) | Window, `GtkPaned`, `GtkListBox`, `GtkHeaderBar`, text view |
| `libsoup-3.0` | WebSocket client (`SoupWebsocketConnection`) |
| `json-glib-1.0` | JSON parse/generate |

### Key files
| File | Role |
|------|------|
| `src/main.cpp` | `GtkApplication` entry, `on_activate` signal |
| `src/models.h` | `Folder`, `Note`, `WireMessage` structs |
| `src/main_window.h/.cpp` | GTK4 window: nested `GtkPaned` 3-column layout, all signal handlers |
| `src/note_view_model.h/.cpp` | `NotesViewModel` — state, selection, debounce, merge, CRUD |
| `src/local_store.h/.cpp` | Read/write `data.json` (folders + notes) via json-glib |
| `src/sync_service.h/.cpp` | libsoup-3 WebSocket client, full wire protocol, auto-reconnect |

### Architecture notes
- All callbacks (libsoup + GTK) fire on the GLib main thread — no explicit thread marshaling needed
- Debounce uses `g_timeout_add(500, ...)` / `g_source_remove()` for cancel-and-restart
- Reconnect uses `g_timeout_add_seconds(3, ...)`
- Four `suppress_*` flags on `MainWindow` prevent feedback loops when programmatically updating widgets
- `GtkListBox` cleared and rebuilt on every folder/note change (simple + correct for this scale)
- Right-click context menus use `GtkGestureClick` (button=3) + `GtkPopover` attached to the row
- `GtkStack` switches editor between "empty" page ("Select a note") and "editor" page
- `gtk_list_box_set_header_func` adds a "Folders" section header in the folder sidebar
- Selection restore after list rebuild: suppress signals → rebuild → call `gtk_list_box_select_row`

---

## iOS (Swift + SwiftUI)

**Status:** Phase 2b complete — folders + multiple notes, three-column layout, running on device
**Location:** `notes/ios/`
**Build:** Open `notes/ios/Notes.xcodeproj` in Xcode, select your device, press Run
**Requires:** Xcode + Apple Developer account (free tier works for personal device)

### What it does
- `NavigationSplitView` three-column layout: folder sidebar | note list | note editor
- Create/rename/delete folders (context menu on folder rows)
- Create/delete notes (toolbar button + context menu)
- Note list sorted by `updatedAt` descending, with title, date, and content preview
- "All Notes" virtual view shows all notes across all folders
- Full offline-first: loads `data.json` on startup, works without server
- Debounce: 500ms after last change to title or content → save locally + push to server
- Per-note last-write-wins merge on reconnect (push local if ahead)
- Auto-reconnect every 3 seconds; green/red status dot in toolbar
- Settings sheet for configurable server URL (stored in `UserDefaults`)
- No `AppDelegate` activation hack — iOS apps are foreground by default

### Local storage
`<App>/Library/Application Support/amadeuz/data.json`
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage
`UserDefaults` — key `"serverAddress"`
Default server: `ws://localhost:8080/ws`

### Key files
| File | Role |
|------|------|
| `NoteApp.swift` | `@main` entry; clean — no AppDelegate hack needed on iOS |
| `Models.swift` | `Folder`, `Note`, `WSMsg` — shared type definitions |
| `ContentView.swift` | `NavigationSplitView` with `FolderSidebar`, `NoteList`, `NoteEditor`, `SettingsView` |
| `NoteViewModel.swift` | `NotesViewModel` — state, selection, debounce, sync, CRUD |
| `LocalStore.swift` | Read/write `data.json` in Application Support |
| `SyncService.swift` | `URLSessionWebSocketTask` wrapper, auto-reconnect (shared logic with macOS) |

### Code sharing with macOS
`LocalStore.swift`, `SyncService.swift`, and `Models.swift` are functionally identical to the macOS versions. The view layer (`ContentView.swift`, `NoteViewModel.swift`) is adapted for touch — alert-based dialogs instead of popover context menus where needed, keyboard type hints in `SettingsView`.

### Known quirks
- `NavigationSplitView` on iPhone collapses to a stack — the three-column layout becomes a drill-down navigation. Works correctly on iPad and in landscape on large iPhones.

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
