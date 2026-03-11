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
| Server persistence (data.json) | ✅ | — | — | — | — | — |
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
| **User accounts / JWT auth** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **End-to-end encryption** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Note sharing between users** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## Server (Go)

**Status:** Phase 2b in progress — multiple notes + folders implemented, no auth yet.
**Location:** `server/`
**Run:** `cd server && go mod tidy && go run .`
**Port:** `8080` on all interfaces (`0.0.0.0`)**Tests:** `cd server && go test ./...` (22 unit tests, all passing)


### Current state (Phase 2b)
- Single WebSocket endpoint at `/ws`, no auth
- Full folder + note CRUD over WebSocket messages (see wire protocol below)
- Notes do not require a folder — `folder_id` is optional on `create_note`
- `move_note` message moves a note to a different folder (or removes it from all folders)
- In-memory store + atomic disk persistence to `server/data.json`
- On new client connect: sends `init` with all folders and all notes (including content)
- Last-write-wins per note (timestamp compare), cascade delete when folder is deleted
- Blob store: `PUT /blobs/:id` (idempotent upload, client-provided UUID), `GET /blobs/:id` (serves with detected content-type, immutable cache headers); max 20 MB per blob; blobs stored in `server/blobs/` directory

### Wire protocol (current)

**Client → Server:**
```json
{ "type": "create_folder", "name": "Work" }                                       ← server generates ID
{ "type": "create_folder", "folder_id": "<client-uuid>", "name": "Work" }         ← client-provided ID (offline sync)
{ "type": "rename_folder", "folder_id": "...", "name": "Work Projects" }
{ "type": "delete_folder", "folder_id": "..." }
{ "type": "create_note",   "title": "My Note" }                                   ← folder_id optional; omit for unfoldered
{ "type": "create_note",   "folder_id": "...", "title": "My Note" }
{ "type": "create_note",   "note_id": "<client-uuid>", "folder_id": "...", "title": "...", "content": "...", "updated_at": 123 }  ← full offline note
{ "type": "update_note",   "note_id": "...", "title": "...", "content": "...", "updated_at": 123456 }
{ "type": "move_note",     "note_id": "...", "folder_id": "..." }                  ← folder_id empty = no folder
{ "type": "delete_note",   "note_id": "..." }
```

`folder_id` and `note_id` on create messages are optional. If provided, the server uses them; if absent, the server generates a UUID. This lets clients create entities locally with a stable ID and push them later without ID conflicts.

**Server → Client (on connect):**
```json
{ "type": "init", "folders": [...], "notes": [...] }
```

**Server → Client (broadcasts after mutations):**
```json
{ "type": "folder_created", "folder": { "id": "...", "name": "...", "created_at": 123 } }
{ "type": "folder_renamed", "folder": { "id": "...", "name": "...", "created_at": 123 } }
{ "type": "folder_deleted", "folder_id": "..." }
{ "type": "note_created",   "note": { "id": "...", "folder_id": "...", "title": "...", ... } }
{ "type": "note_updated",   "note": { "id": "...", "folder_id": "...", "title": "...", ... } }
{ "type": "note_moved",     "note": { "id": "...", "folder_id": "...", ... } }
{ "type": "note_deleted",   "note_id": "..." }
```

Create responses are sent to originating client **and** broadcast to others.
If the client already has the entity (matched by ID), the echo is a no-op.
Update/move/delete responses are broadcast to others only.

### File layout
```
server/
├── main.go        ← HTTP setup + WebSocket upgrade loop + blob route
├── model.go       ← Folder, Note, Msg types
├── store.go       ← in-memory store + atomic JSON persistence
├── hub.go         ← WebSocket hub + message dispatch
├── blobs.go       ← PUT/GET /blobs/:id — idempotent upload, content-type detection
└── store_test.go  ← 20 unit tests
```

### Persistence
- File: `server/data.json`
- Format: `{ "folders": [...], "notes": [...] }`
- Writes are atomic (write to temp file, then `os.Rename`) — no partial-write corruption
- Writes are synchronous per mutation (fast enough; avoids race conditions from async goroutines)

### Deployment (Raspberry Pi)
- Architecture: `aarch64` (64-bit ARM)
- Build on Mac: `GOOS=linux GOARCH=arm64 go build -o amadeuz-server .`
- Copy: `scp amadeuz-server pi@<PI_IP>:/opt/amadeuz/amadeuz-server`
- Binary location: `/opt/amadeuz/amadeuz-server`
- Managed by systemd: `sudo systemctl restart amadeuz`
- Logs: `sudo journalctl -u amadeuz -f`

### Dependencies (current)
- `github.com/gorilla/websocket`

### Dependencies (planned additions)
- `modernc.org/sqlite` — pure Go SQLite, no CGO
- `go-chi/chi` — lightweight router with URL parameters
- `golang-jwt/jwt` — JWT sign/verify

---

## macOS (Swift + SwiftUI)

**Status:** Phase 2b complete + offline-first create + inline images + Markdown styling (2026-03-11)
**Location:** `notes/mac/`
**Run:** `cd notes/mac && swift run`
**Requires:** Xcode command-line tools + accepted license (`sudo xcodebuild -license`)

### What it does
- Three-column `NavigationSplitView`: folder list | note list | note editor
- Create/rename/delete folders (context menu on folder rows)
- Create/delete notes (toolbar button + context menu)
- Notes do not require a folder — can be created from "All Notes" view directly
- Move note to a different folder (or to no folder) via right-click → "Move to Folder" submenu
- Note list sorted by `updatedAt` descending, with title, date, content preview, and folder label
- Folder label shown in note row (folder icon + name); shows "—" if note has no folder (keeps row height constant)
- Search field (top-right toolbar) filters notes by title or content; auto-switches to "All Notes" when typing
- Rich Markdown editor (`MarkdownEditor.swift`): inline images via drag & drop or paste, Markdown styling for headers (`#`, `##`, `###`), bullets (`-`), and checkboxes (`- [ ]`, `- [x]`)
- Images stored as `![](amadeuz://blob/<uuid>)` in note content; binary data managed by `BlobStore`
- Offline image support: blob saved locally and queued for upload the moment it is inserted; upload fires in background and retries on reconnect
- Full offline-first: loads `data.json` on startup, works without server
- Create notes and folders while offline — they appear immediately, saved to disk, pushed to server on reconnect
- Debounce: 500ms after last change to title or content → save locally + push to server
- Per-note last-write-wins merge on reconnect (push local if ahead)
- Local-only folders and notes created offline are pushed to server on reconnect (not dropped)
- Auto-reconnect every 3 seconds; green/red status dot in toolbar

### Local storage
`~/Library/Application Support/amadeuz/data.json`
```json
{ "folders": [...], "notes": [...] }
```

### Settings storage
`UserDefaults` — key `"serverAddress"`
Default server: `ws://localhost:8080/ws`

### Key files
| File | Role |
|------|------|
| `NoteApp.swift` | `@main` entry; `AppDelegate` forces foreground activation (SPM quirk) |
| `Models.swift` | `Folder`, `Note`, `WSMsg` — all types + Codable conformance |
| `ContentView.swift` | `NavigationSplitView` with `FolderSidebar`, `NoteList`, `NoteEditor` |
| `NoteViewModel.swift` | `NotesViewModel` — state, selection management, debounce, sync |
| `MarkdownEditor.swift` | `NSTextView`-based rich editor: inline images (paste/drop), Markdown styling, blob insertion |
| `BlobStore.swift` | Local blob cache (`~/Library/Application Support/amadeuz/blobs/`), pending upload queue, `PUT /blobs/:id` HTTP sync |
| `LocalStore.swift` | Read/write `data.json` (folders + notes) in Application Support |
| `SyncService.swift` | `URLSessionWebSocketTask` wrapper, generic `WSMsg` handler, auto-reconnect |

### Selection + debounce design
- `NotesViewModel` tracks `editingNoteID` (private) separate from `selectedNoteID` (published)
- `noteSelectionChanged(from:to:)` flushes pending changes to outgoing note, loads incoming note
- Called from view's `.onChange(of: vm.selectedNoteID)` — view is the trigger, not Combine
- Debounce uses `Publishers.CombineLatest($editingTitle, $editingContent)` — fires 500ms after last change to either field

### Known quirks
- SPM executables don't activate as foreground apps by default. Fixed with `AppDelegate`:
  `NSApp.setActivationPolicy(.regular)` + `NSApp.activate(ignoringOtherApps: true)`

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
