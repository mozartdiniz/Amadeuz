# Amadeuz Notes — Android

Native Android notes app built with Kotlin + Jetpack Compose. Offline-first, syncs over WebSocket.

## Requirements

- [Android Studio](https://developer.android.com/studio) (Hedgehog 2023.1.1 or later recommended)
- JDK 11+ (bundled with Android Studio)
- Android SDK with API level 36 (compile SDK) — install via Android Studio's SDK Manager
- Minimum supported Android version: API 24 (Android 7.0 Nougat)

### Install Android SDK

1. Open Android Studio → **SDK Manager** (via **Settings > Languages & Frameworks > Android SDK**)
2. In the **SDK Platforms** tab, install **Android API 36**
3. In the **SDK Tools** tab, ensure **Android SDK Build-Tools** and **Android Emulator** are installed

## Build & Run

### Using Android Studio (recommended)

1. Open the project:

```
File > Open > notes/android
```

2. Wait for Gradle sync to complete (downloads dependencies automatically)
3. Select a device or emulator in the toolbar
4. Press **Shift+F10** (or the Run button) to build and run

### Using Gradle on the command line

```bash
cd notes/android

# Run on a connected device or emulator
./gradlew installDebug
adb shell am start -n com.amadeuz.notes/.MainActivity

# Or just build an APK
./gradlew assembleDebug
# Output: app/build/outputs/apk/debug/app-debug.apk
```

On Windows use `gradlew.bat` instead of `./gradlew`.

## Dependencies (managed by Gradle)

| Library | Purpose |
|---------|---------|
| Jetpack Compose + Material3 | UI |
| AndroidX Lifecycle | ViewModel + coroutines |
| OkHttp | WebSocket client |
| Gson | JSON serialization |

All dependencies are declared in `app/build.gradle.kts` and resolved from Maven Central / Google.

## Connecting to the Server

1. Start the sync server: `cd server && go run .` (listens on port `8080`)
2. Open the Settings screen in the app and enter the server WebSocket URL, e.g. `ws://192.168.1.x:8080/ws`
3. Ensure the Android device/emulator and the server are on the same network

> For an emulator connecting to a server running on the host machine, use `ws://10.0.2.2:8080/ws` instead of `localhost`.

## Network Permission

The app requires the `INTERNET` permission (declared in `AndroidManifest.xml`). For plain `ws://` (non-TLS) connections on Android 9+, a Network Security Config entry is needed for your server's IP — check `app/src/main/res/xml/network_security_config.xml` if plain WebSocket connections are refused.

## Local Storage

Notes are stored in the app's internal storage:

```
<AppDataDir>/note.json
```

Format:

```json
{ "content": "...", "updatedAt": 1234567890123 }
```

## Project Structure

```
app/src/main/
├── java/com/amadeuz/notes/
│   ├── MainActivity.kt        # Entry point, Compose host
│   ├── NoteViewModel.kt       # State, debounce, sync logic, offline-first merge
│   ├── LocalStore.kt          # Read/write note.json in internal storage
│   ├── SyncService.kt         # OkHttp WebSocket client, auto-reconnect
│   └── Models.kt              # Shared data types
├── res/                       # Resources (layouts, strings, icons)
└── AndroidManifest.xml
```
