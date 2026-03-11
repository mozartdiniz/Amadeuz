# Amadeuz Notes — Windows

Native Windows notes app built with C# + WinUI 3. Offline-first, syncs over WebSocket.

## Requirements

- Windows 10 version 1809 (build 17763) or later; Windows 11 recommended
- [Visual Studio 2022](https://visualstudio.microsoft.com/) with the following workloads:
  - **.NET desktop development**
  - **Windows application development** (includes the Windows App SDK / WinUI 3 tools)
- .NET 9 SDK (or .NET 10 if you update the `TargetFramework` in `Amadeuz.csproj`)
- [Windows App Runtime 1.8](https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads) — installed automatically via NuGet when you build

### Install Visual Studio Workloads

In the Visual Studio Installer, select:

1. **.NET desktop development**
2. **Windows application development**

Both are needed. The second one pulls in the WinUI 3 project templates and the Windows App SDK build tools.

## Build & Run

### Using Visual Studio (recommended)

1. Open the solution:

```
notes/windows/Amadeuz.sln
```

2. Set the platform to **x64** in the toolbar
3. Press **F5** (or **Ctrl+F5** for run without debugger)

NuGet will restore `Microsoft.WindowsAppSDK` and `Microsoft.Windows.SDK.BuildTools` automatically on first build.

### Using the .NET CLI

```powershell
cd notes\windows
dotnet restore
dotnet build -c Release
dotnet run
```

> The app targets `win-x64` and is unpackaged (no MSIX). The Windows App Runtime bootstrapper initializes automatically at startup.

## Connecting to the Server

1. Start the sync server: `cd server && go run .` (listens on port `8080`)
2. Open the Settings panel in the app and enter the server WebSocket URL, e.g. `ws://192.168.1.x:8080/ws`
3. The status indicator will turn green when connected

## Local Storage

Notes are stored at:

```
%APPDATA%\amadeuz\note.json
```

Format:

```json
{ "content": "...", "updatedAt": 1234567890123 }
```

## Project Structure

```
Amadeuz/
├── App.xaml / App.xaml.cs         # Application entry point, bootstrapper init
├── MainWindow.xaml / .cs          # WinUI 3 window, text box, status bar, settings
├── NotesViewModel.cs              # State, debounce, sync logic, offline-first merge
├── LocalStore.cs                  # Read/write note.json in %APPDATA%
├── SyncService.cs                 # Windows WebSocket client, auto-reconnect
├── Models.cs                      # Shared data types
├── Package.appxmanifest           # Required by WinUI 3 even for unpackaged apps
└── Amadeuz.csproj
```
