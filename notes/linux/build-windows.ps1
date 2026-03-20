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

# 2. Check Rust target
Write-Step "Checking dependencies..."
$installedTargets = rustup target list --installed
if ($installedTargets -notmatch $TARGET) {
    Write-Host "   Installing Rust target $TARGET..." -ForegroundColor Yellow
    rustup target add $TARGET
}
Write-Ok "Rust target $TARGET is ready."

# 3. Build
Write-Step "Compiling for $TARGET..."
cargo build --release --target $TARGET
Write-Ok "Build complete."

# 4. Collect output
Write-Step "Collecting output..."
if (!(Test-Path $BUILD_DIR)) { New-Item -ItemType Directory -Path $BUILD_DIR | Out-Null }
Copy-Item "target\$TARGET\release\amadeuz-notes.exe" -Destination $OUTPUT -Force
Copy-Item "$GTK_ROOT\bin\*.dll" -Destination $BUILD_DIR -Force
Write-Ok "Binary: $OUTPUT"
Write-Ok "GTK DLLs copied from $GTK_ROOT\bin\"

Write-Host "`nDone!" -ForegroundColor Green
Write-Host "Run: .\$BUILD_DIR\amadeuz-notes.exe"
