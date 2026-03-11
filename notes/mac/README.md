# Amadeuz Notes — macOS

Native macOS notes app built with Swift + SwiftUI. Offline-first, syncs over WebSocket.

## Requirements

- macOS 14 (Sonoma) or later
- Xcode Command Line Tools

### Install Xcode Command Line Tools

```bash
xcode-select --install
```

If you've never agreed to the Xcode license:

```bash
sudo xcodebuild -license
```

## Build & Run

```bash
cd notes/mac
swift run
```

Swift Package Manager will resolve dependencies and launch the app automatically.

## What It Does

- Loads the last saved note from disk on startup (works fully offline)
- Connects to the sync server via WebSocket and auto-reconnects every 3 seconds on failure
- On connect: compares local vs server timestamp and keeps whichever is newer
- Debounces saves: 500ms after the last keystroke → writes to disk and sends to server
- Status bar shows green "Synced" or red "Offline"
- Server URL is configurable in the Settings sheet and persisted in `UserDefaults`

## Local Storage

Notes are stored at:

```
~/Library/Application Support/amadeuz/note.json
```

Format:

```json
{ "content": "...", "updatedAt": 1234567890123 }
```

## Connecting to the Server

1. Start the sync server: `cd server && go run .` (listens on port `8080`)
2. Open Settings in the app and set the WebSocket URL, e.g. `ws://192.168.1.x:8080/ws`
3. The status bar will turn green when connected

## Project Structure

```
Sources/Notes/
├── NoteApp.swift          # @main entry point, AppDelegate for window focus
├── ContentView.swift      # TextEditor UI, status bar, settings sheet
├── NoteViewModel.swift    # State, debounce, sync logic, offline-first merge
├── LocalStore.swift       # Read/write note.json in Application Support
└── SyncService.swift      # URLSessionWebSocketTask wrapper, auto-reconnect
```
