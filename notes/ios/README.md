# Amadeuz Notes — iOS

Native iOS notes app built with Swift + SwiftUI. Offline-first, syncs over WebSocket.

## Requirements

- macOS with Xcode 15 or later
- iOS 17 SDK (included with Xcode 15+)
- An iOS device or the iOS Simulator

### Install Xcode

Download Xcode from the [Mac App Store](https://apps.apple.com/app/xcode/id497799835) or the Apple Developer portal.

## Build & Run

### Using Xcode (recommended)

1. Open the project in Xcode:

```bash
cd notes/ios
open Notes.xcodeproj
```

2. Select your target device or simulator in the toolbar
3. Press **Cmd+R** to build and run

### Using Swift Package Manager (Simulator only)

```bash
cd notes/ios
swift build
```

> Note: Running on a physical device requires signing configuration in Xcode — set your Team in **Signing & Capabilities**.

## Connecting to the Server

1. Start the sync server: `cd server && go run .` (listens on port `8080`)
2. Open the Settings screen in the app and enter the server WebSocket URL, e.g. `ws://192.168.1.x:8080/ws`
3. Ensure the iOS device and the server are on the same Wi-Fi network

> iOS enforces ATS (App Transport Security). For local development over plain `ws://`, you may need to add an ATS exception in `Info.plist` for your server's IP address, or use a server with TLS (`wss://`).

## Local Storage

Notes are stored in the app's Application Support directory on-device:

```
<AppSandbox>/Library/Application Support/amadeuz/note.json
```

Format:

```json
{ "content": "...", "updatedAt": 1234567890123 }
```

## Project Structure

```
Sources/Notes/
├── NoteApp.swift          # @main entry point
├── ContentView.swift      # TextEditor UI, status bar, settings sheet
├── NoteViewModel.swift    # State, debounce, sync logic, offline-first merge
├── LocalStore.swift       # Read/write note.json
└── SyncService.swift      # URLSessionWebSocketTask wrapper, auto-reconnect
```
