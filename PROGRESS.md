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

| Feature | Server | macOS | Windows | Linux | iOS |
|---------|:------:|:-----:|:-------:|:-----:|:---:|
| Single note editor | — | ✅ | ✅ | ✅ | ❌ |
| Offline-first local storage | — | ✅ | ✅ | ✅ | ❌ |
| WebSocket connection to server | ✅ | ✅ | ✅ | ✅ | ❌ |
| Auto-reconnect (3 s) | — | ✅ | ✅ | ✅ | ❌ |
| 500 ms debounce save | — | ✅ | ✅ | ✅ | ❌ |
| Timestamp merge (last-write-wins) | ✅ | ✅ | ✅ | ✅ | ❌ |
| Push local-ahead note on connect | — | ✅ | ✅ | ✅ | ❌ |
| Connection status indicator | — | ✅ | ✅ | ✅ | ❌ |
| Configurable server address | — | ✅ | ✅ | ✅ | ❌ |
| Persistent server address | — | ✅ | ✅ | ✅ | ❌ |
| Server persistence (note.json) | ✅ | — | — | — | — |
| **User accounts / JWT auth** | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Multiple notes** | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Folders** | ❌ | ❌ | ❌ | ❌ | ❌ |
| **End-to-end encryption** | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Note sharing between users** | ❌ | ❌ | ❌ | ❌ | ❌ |

---

## Server (Go)

**Status:** Phase 1 POC complete (single note, no auth). Deployed on Raspberry Pi. Phase 2 (modular monolith) in design.
**Location:** `server/`
**Run:** `cd server && go mod tidy && go run .`
**Port:** `8080` on all interfaces (`0.0.0.0`)

### Current state (Phase 1)
- Single WebSocket endpoint at `/ws`, no auth
- In-memory note + disk persistence to `server/note.json`
- On new client connect: sends current note as `init` message
- On `update` from any client: stores if newer (timestamp compare), broadcasts to all others

### Target structure (Phase 2 — modular monolith)

```
server/
├── main.go
└── internal/
    ├── config/     ← port, DB path, JWT secret, feature flags
    ├── db/         ← SQLite (modernc.org/sqlite), migrations
    ├── auth/       ← POST /auth/register, /auth/login, JWT middleware
    ├── users/      ← user model, public key storage
    ├── notes/      ← REST CRUD + per-note WebSocket hub
    ├── folders/    ← folder CRUD
    └── hub/        ← WebSocket rooms keyed by note ID
```

### Database schema (planned)

```sql
users(id, email, password_hash, public_key, created_at)
folders(id, owner_id, name, created_at, updated_at)
notes(id, owner_id, folder_id, title, content, updated_at, created_at)
note_keys(note_id, user_id, ephemeral_pub, wrapped_key, nonce)
note_shares(note_id, owner_id, shared_with, can_edit, created_at)
```

### Key files
- `server/main.go` — currently everything; will become thin wiring layer

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

**Status:** POC complete
**Location:** `notes/mac/`
**Run:** `cd notes/mac && swift run`
**Requires:** Xcode command-line tools + accepted license (`sudo xcodebuild -license`)

### What it does
All POC features. See feature matrix above.

### Local storage
`~/Library/Application Support/amadeuz/note.json`

### Settings storage
`UserDefaults` — key `"serverAddress"`
Default server: `ws://localhost:8080/ws`

### Key files
| File | Role |
|------|------|
| `NoteApp.swift` | `@main` entry; `AppDelegate` forces foreground activation (SPM quirk) |
| `ContentView.swift` | `TextEditor` + status bar + settings sheet |
| `NoteViewModel.swift` | State, debounce (Combine), sync logic, offline-first merge |
| `LocalStore.swift` | Read/write `note.json` in Application Support |
| `SyncService.swift` | `URLSessionWebSocketTask` wrapper, auto-reconnect |

### Known quirks
- SPM executables don't activate as foreground apps by default. Fixed with `AppDelegate`:
  `NSApp.setActivationPolicy(.regular)` + `NSApp.activate(ignoringOtherApps: true)`

---

## Windows (C# + WinUI 3)

**Status:** POC complete
**Location:** `notes/windows/`
**Build:** Open `Amadeuz.sln` in Visual Studio 2022, press F5
**Requires:** Visual Studio 2022 + Windows App SDK workload, Windows 11

### What it does
All POC features. See feature matrix above.

### Local storage
`%APPDATA%\amadeuz\note.json`

### Settings storage
`%APPDATA%\amadeuz\settings.json` — JSON `{ "serverAddress": "ws://..." }`
Default server: `ws://localhost:8080/ws`

### Key files
| File | Role |
|------|------|
| `MainWindow.xaml` | WinUI layout: `TextBox` + status bar (dot + label + Settings button) |
| `MainWindow.xaml.cs` | Event wiring; `_suppressTextChanged` flag prevents echo loop |
| `NoteViewModel.cs` | State, debounce (`CancellationTokenSource` + `Task.Delay`), merge logic |
| `LocalStore.cs` | Read/write `note.json` via `System.Text.Json` |
| `SyncService.cs` | `ClientWebSocket` wrapper, connect loop, auto-reconnect |

### Visual notes
- Uses Mica backdrop (`SystemBackdrop = new MicaBackdrop()`) for native Windows 11 look
- Status dot is a WinUI `Ellipse` with `Fill` changed in code-behind
- Settings dialog is a `ContentDialog` built entirely in code (no XAML)

---

## Linux (C++ + GTK4)

**Status:** POC complete
**Location:** `notes/linux/`

### Build

```bash
# Install dependencies (Ubuntu / Debian)
sudo apt install cmake build-essential libgtk-4-dev libsoup-3.0-dev libjson-glib-dev

# Configure + build
cd notes/linux
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build

# Run
./build/amadeuz
```

### What it does
All POC features. See feature matrix above.

### Local storage
`~/.local/share/amadeuz/note.json` (XDG_DATA_HOME)

### Settings storage
`~/.local/share/amadeuz/settings.json` — JSON `{ "serverAddress": "ws://..." }`
Default server: `ws://localhost:8080/ws`

### Dependencies
| Library | Used for |
|---------|----------|
| `gtk4` | Window, text view, widgets |
| `libsoup-3.0` | WebSocket client (`SoupWebsocketConnection`) |
| `json-glib-1.0` | JSON parse/generate |

### Key files
| File | Role |
|------|------|
| `src/main.cpp` | `GtkApplication` entry, `on_activate` signal |
| `src/main_window.h/.cpp` | GTK4 window: `GtkTextView` + separator + status bar + settings dialog |
| `src/note_view_model.h/.cpp` | State, debounce (`g_timeout_add`), merge logic, settings load/save |
| `src/local_store.h/.cpp` | Read/write `note.json` via json-glib |
| `src/sync_service.h/.cpp` | libsoup-3 WebSocket client, auto-reconnect via `g_timeout_add_seconds` |

### Architecture notes
- All callbacks (libsoup + GTK) fire on the GLib main thread — no explicit thread marshaling needed
- Debounce uses `g_timeout_add(500, ...)` / `g_source_remove()` for cancel-and-restart
- Reconnect uses `g_timeout_add_seconds(3, ...)`
- `suppress_changed_` flag on `MainWindow` prevents server-received text from re-triggering debounce
- Settings dialog is a plain `GtkWindow` (modal) — avoids deprecated `GtkDialog`

---

## iOS (Swift + SwiftUI)

**Status:** Not started
**Location to create:** `notes/ios/`

### Plan
- Will share `LocalStore.swift` and `SyncService.swift` logic with the macOS client
- UI adapted for mobile: full-screen text editor, settings via system Settings or in-app sheet
- Local storage: app's Documents or Application Support directory
- No `AppDelegate` activation hack needed on iOS
