# Amadeuz — Project Brief for Claude

## Vision

Open-source iCloud replacement, starting with a **cloud-synced Notes app**.
Long-term: files, photos, contacts, calendar — all self-hosted.

---

## Platform Strategy

No browser engine wrappers (no Electron, Tauri, WebView). Everything is native.
Multiple platform-specific clients are acceptable. Minimize where possible but not at the cost of native feel.

| Platform | Language / Framework | Status |
|----------|---------------------|--------|
| Server   | Go                  | Done (POC) |
| macOS    | Swift + SwiftUI     | Done (POC) |
| Windows  | C++ + WinUI 3       | Not started |
| Linux    | C++ or Rust + GTK4  | Not started |
| iOS      | Swift + SwiftUI     | Not started (shares code with macOS) |

---

## Repository Layout

```
amadeuz/
├── CLAUDE.md
├── .gitignore
├── server/                   ← Go sync server
│   ├── go.mod
│   ├── go.sum
│   └── main.go
└── notes/
    └── mac/                  ← macOS SwiftUI app
        ├── Package.swift
        └── Sources/Notes/
            ├── NoteApp.swift
            ├── ContentView.swift
            ├── NoteViewModel.swift
            ├── LocalStore.swift
            └── SyncService.swift
```

---

## Server (Go) — Done

**Location:** `server/`
**Run:** `cd server && go mod tidy && go run .`
**Port:** `8080` on all interfaces (`0.0.0.0`) — accepts LAN connections

### What it does
- WebSocket endpoint at `/ws`
- Stores the latest note content in memory + persists to `server/note.json`
- On new client connect: sends current note as `init` message
- On update from any client: stores if newer (timestamp compare), broadcasts to all other clients
- Last-write-wins using Unix millisecond timestamps

### Wire protocol

All messages are JSON over WebSocket.

```json
// Server → client (on connect)
{ "type": "init", "content": "...", "updated_at": 1234567890123 }

// Client → server (on change)
{ "type": "update", "content": "...", "updated_at": 1234567890123 }

// Server → all other clients (after accepting an update)
{ "type": "update", "content": "...", "updated_at": 1234567890123 }
```

`updated_at` is Unix milliseconds. Server only accepts and broadcasts an update if its
timestamp is strictly greater than the currently stored one.

---

## macOS App (Swift + SwiftUI) — Done

**Location:** `notes/mac/`
**Run:** `cd notes/mac && swift run`
**Requires:** Xcode command-line tools + agreed license (`sudo xcodebuild -license`)

### What it does
- Full offline-first: loads last known note from local disk on startup, works without server
- Connects to server via WebSocket; auto-reconnects every 3 seconds on failure
- On connect: compares local timestamp vs server timestamp, keeps newer, pushes local if ahead
- Debounces saves: 500ms after last keystroke → saves to disk + sends to server
- Status bar: green "Synced" / red "Offline"
- Settings sheet: configurable server WebSocket URL (stored in UserDefaults)

### Local storage
`~/Library/Application Support/amadeuz/note.json`
```json
{ "content": "...", "updatedAt": 1234567890123 }
```

### Key files
- `NoteApp.swift` — `@main` entry, AppDelegate activates window focus on launch
- `ContentView.swift` — TextEditor + status bar + settings sheet
- `NoteViewModel.swift` — state, debounce, sync logic, offline-first merge
- `LocalStore.swift` — read/write `note.json` in Application Support
- `SyncService.swift` — URLSessionWebSocketTask wrapper, auto-reconnect

### Known issue fixed
SPM executables don't auto-activate as foreground apps. Fixed with `AppDelegate`:
```swift
NSApp.setActivationPolicy(.regular)
NSApp.activate(ignoringOtherApps: true)
```

---

## Windows App (C++ + WinUI 3) — Not started

**Location to create:** `notes/windows/`

### What to build
Functional equivalent of the macOS app:
- Native WinUI 3 window with a text editing area
- WebSocket client (Windows.Networking.Sockets or WinHTTP/WinRT WebSocket API)
- Local persistence to `%APPDATA%\amadeuz\note.json` (same JSON format as Mac)
- Same offline-first logic: load local → connect → merge by timestamp → push if ahead
- Same debounce pattern: 500ms after last keystroke → save + send
- Status indicator: connected / offline
- Configurable server address (stored in app settings / registry)

### Suggested project setup
- Visual Studio 2022 on Windows 11
- New project: **Blank App, Packaged (WinUI 3 in Desktop)**
- Language: C++
- The WinRT WebSocket API (`Windows::Networking::Sockets::MessageWebSocket`) is available
  in C++ without any extra dependencies

### Success criteria for the POC
1. Server running on one machine
2. Mac app open, connected, some text typed
3. Windows app open on another machine pointing at same server
4. Text appears on Windows without any manual action — and vice versa

---

## Linux App — Not started

**Location to create:** `notes/linux/`

Decision: C++ + GTK4 or Rust + gtk4-rs. Defer until Mac + Windows POC is validated.

---

## iOS App — Not started

Swift + SwiftUI. Will share `LocalStore.swift` and `SyncService.swift` logic with the macOS app.
Defer until desktop POC is validated.

---

## What's Missing for the POC to be Complete

- [ ] Windows client (`notes/windows/`) — the immediate next step
- [ ] Test end-to-end: Mac ↔ Windows over LAN
- [ ] Linux client (`notes/linux/`)
- [ ] iOS client (`notes/ios/`)

## Future (post-POC)

- [ ] Multiple notes (not just one shared document)
- [ ] User accounts / auth
- [ ] Conflict resolution beyond last-write-wins
- [ ] End-to-end encryption
- [ ] File sync (the broader iCloud replacement scope)
- [ ] Server deployment beyond LAN (DNS, HTTPS/WSS, dynamic IP handling)

---

## Living Documents — Rules for Claude

Three markdown files must be kept up to date **automatically**, without being asked.
Update them at the end of every session in which anything relevant happened.

---

### VISION.md

**Purpose:** Strategic and architectural reference. Decision-level document.
Used as a guide when evaluating technical choices — "does this align with the vision?"

**What belongs here:**
- The end-goal of the project and core principles (native-everywhere, offline-first, self-hosted, etc.)
- Architecture diagrams and wire protocol specification
- Platform and technology decisions with rationale
- Roadmap phases
- The decisions log — *every* non-trivial technical choice and *why* it was made

**Rules:**
- Update whenever a technology, architecture, or design decision is made or changed
- Update the decisions log immediately when a decision is made — while the reasoning is fresh
- This file must remain consistent with what is actually implemented; never let it drift
- Tone: polished, reference-quality. This is the document a new contributor reads to understand the project

---

### PROGRESS.md

**Purpose:** Practical state tracker for cross-platform work. Machine-readable by Claude.

**What belongs here:**
- The feature matrix table (features vs platforms, ✅ / ❌)
- Per-platform status, build instructions, file structure, key files, and quirks
- Enough context that opening a new AI session with "read PROGRESS.md, I'm working on [platform]"
  is sufficient to resume work without any further explanation

**Rules:**
- Update the feature matrix immediately when a feature is completed on any platform
- Update a platform's section whenever its status, files, build steps, or known issues change
- Mark a platform "POC complete" only when all POC features in the matrix are ✅
- This file is the source of truth for "what is done and where"

---

### DIARY.md

**Purpose:** Raw build journal. Source material for blog posts.

**What belongs here:**
- Every significant session: what was built, what decisions were made, what problems were hit
- The *reasoning* behind technical choices, written while it's still fresh
- `> 📝` blockquotes as prompts for the human author to fill in personal reflections
- Honest accounts of mistakes, surprises, and pivots — these make for good writing
- A "where things stand" summary at the end of each day or milestone

**Rules:**
- Add a new dated entry at the end of every session that made meaningful progress
- Write in the established voice: plain technical prose, no marketing language, no oversimplification
- Do not rewrite or clean up past entries — append only
- Leave `> 📝` prompts wherever the human's personal perspective would strengthen the eventual blog post
- The audience for the final blog posts is developers of all levels, including beginners
  Keep explanations honest and accessible without being condescending
