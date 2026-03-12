# Amadeuz Notes — Linux

Native Linux notes app built with C++20 + GTK4. Offline-first, syncs over WebSocket.

## Requirements

- GCC 12+ or Clang 15+ with C++20 support
- CMake 3.20+
- GTK 4
- libsoup 3 (WebSocket client)
- json-glib
- libsecret (keychain integration)

### Install Dependencies

**Ubuntu / Debian:**

```bash
sudo apt update
sudo apt install \
  build-essential \
  cmake \
  pkg-config \
  libgtk-4-dev \
  libsoup-3.0-dev \
  libjson-glib-dev \
  libsecret-1-dev
```

**Fedora / RHEL:**

```bash
sudo dnf install \
  gcc-c++ \
  cmake \
  pkgconfig \
  gtk4-devel \
  libsoup3-devel \
  json-glib-devel \
  libsecret-devel
```

**Arch Linux:**

```bash
sudo pacman -S \
  base-devel \
  cmake \
  gtk4 \
  libsoup3 \
  json-glib \
  libsecret
```

## Build & Run

```bash
cd notes/linux
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j$(nproc)
./build/amadeuz
```

For a debug build:

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build -j$(nproc)
```

## Connecting to the Server

1. Start the sync server: `cd server && go run .` (listens on port `8080`)
2. Open the Settings dialog in the app and enter the server WebSocket URL, e.g. `ws://192.168.1.x:8080/ws`
3. The status indicator will show connected when the server is reachable

## Local Storage

Notes are stored at:

```
~/.local/share/amadeuz/note.json
```

Format:

```json
{ "content": "...", "updatedAt": 1234567890123 }
```

## Project Structure

```
src/
├── main.cpp               # Entry point, GTK application setup
├── main_window.cpp/h      # GTK4 window, text view, status bar, settings
├── note_view_model.cpp/h  # State, debounce, sync logic, offline-first merge
├── local_store.cpp/h      # Read/write note.json
├── sync_service.cpp/h     # libsoup WebSocket client, auto-reconnect
└── models.h               # Shared data types
CMakeLists.txt
```
