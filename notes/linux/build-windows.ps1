# Stop on error
$ErrorActionPreference = "Stop"

$TARGET    = "x86_64-pc-windows-msvc"
$BUILD_DIR = "build-windows"
$OUTPUT    = "$BUILD_DIR\amadeuz-notes.exe"
$GTK_ROOT  = "C:\gtk-build\gtk\x64\release"

# --- GTK paths ---
$env:PKG_CONFIG_PATH = "$GTK_ROOT\lib\pkgconfig"
$env:LIB             = "$GTK_ROOT\lib;$env:LIB"
$env:INCLUDE         = "$GTK_ROOT\include;$env:INCLUDE"
$env:Path            = "$GTK_ROOT\bin;$env:Path"

# --- Compile-time config (normally injected by Meson) ---
$env:MESON_APP_ID  = "com.amadeuz.Notes"
$env:MESON_PATH_ID = "/com/amadeuz/Notes"
$env:MESON_PKGNAME = "amadeuz-notes"
$env:MESON_VERSION = "0.1.0"
$env:MESON_PROFILE = "default"
$env:MESON_DATADIR = ""

# Helper functions
function Write-Step { Write-Host "`n>> $($args[0])" -ForegroundColor Cyan }
function Write-Ok   { Write-Host "   OK: $($args[0])" -ForegroundColor Green }
function Write-Err  { Write-Host "   ERROR: $($args[0])" -ForegroundColor Red; exit 1 }

# 1. Verify we're in the right directory
if (!(Test-Path "Cargo.toml")) {
    Write-Err "Run this script from the notes/linux/ directory."
}

# 2. Check dependencies
Write-Step "Checking dependencies..."

$installedTargets = rustup target list --installed
if ($installedTargets -notmatch $TARGET) {
    Write-Host "   Installing Rust target $TARGET..." -ForegroundColor Yellow
    rustup target add $TARGET
}
Write-Ok "Rust target $TARGET is ready."

if (!(Get-Command "cargo-packager" -ErrorAction SilentlyContinue)) {
    Write-Host "   Installing cargo-packager..." -ForegroundColor Yellow
    cargo install cargo-packager --locked
}
Write-Ok "cargo-packager is ready."

# 3. Build
Write-Step "Compiling for $TARGET..."
cargo build --release --target $TARGET
Write-Ok "Build complete."

# 4. Stage binary for cargo-packager
# cargo-packager looks in target\release\, not target\{triple}\release\.
# This copy is the bridge that prevents the LGHT0103 WiX linker error.
Write-Step "Staging binary for packager..."
if (!(Test-Path "target\release")) { New-Item -ItemType Directory "target\release" | Out-Null }
Copy-Item "target\$TARGET\release\amadeuz-notes.exe" -Destination "target\release\amadeuz-notes.exe" -Force
Write-Ok "Binary staged in target\release\"

# 5. MSI installer
Write-Step "Generating MSI installer..."
# Add WiX to PATH for this session if installed in the default location
if (Test-Path "C:\Program Files (x86)\WiX Toolset v3.11\bin") {
    $env:Path += ";C:\Program Files (x86)\WiX Toolset v3.11\bin"
}
cargo packager --release
Write-Ok "MSI installer generated."

# 6. Portable folder (for quick testing without installing)
Write-Step "Building portable folder..."
if (!(Test-Path $BUILD_DIR)) { New-Item -ItemType Directory -Path $BUILD_DIR | Out-Null }
Copy-Item "target\release\amadeuz-notes.exe" -Destination $OUTPUT -Force
Copy-Item "$GTK_ROOT\bin\*.dll" -Destination $BUILD_DIR -Force
Write-Ok "Portable build: $OUTPUT"

Write-Host "`nDone!" -ForegroundColor Green
Write-Host "  Portable:  .\$BUILD_DIR\amadeuz-notes.exe"
Write-Host "  Installer: target\release\amadeuz-notes_0.1.0_x64_en-US.msi" -ForegroundColor Yellow
