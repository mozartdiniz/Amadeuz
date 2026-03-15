#!/usr/bin/env bash
# Build and install Amadeuz Notes as a user Flatpak.
# Run this from anywhere — it resolves its own location.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/com.amadeuz.Notes.yaml"
BUILD_DIR="$SCRIPT_DIR/.flatpak-build"

# ── 1. Prerequisites ──────────────────────────────────────────────────────────

if ! command -v flatpak-builder &>/dev/null; then
    echo "Installing flatpak-builder..."
    sudo dnf install -y flatpak-builder
fi

if ! command -v cargo &>/dev/null; then
    echo "cargo not found — install Rust via rustup or dnf install cargo"
    exit 1
fi

# ── 2. Flatpak runtimes / SDK ─────────────────────────────────────────────────

echo "Ensuring runtimes are installed..."
flatpak install -y --or-update flathub \
    org.gnome.Platform//49 \
    org.gnome.Sdk//49 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08

# ── 3. Vendor dependencies (cargo fetches everything before the sandbox) ──────

if [ ! -d "$SCRIPT_DIR/vendor" ]; then
    echo "Vendoring Cargo dependencies..."
    cd "$SCRIPT_DIR"
    mkdir -p .cargo
    cargo vendor vendor/ > .cargo/config.toml
    echo "vendor/ created."
else
    echo "vendor/ already exists, skipping. Delete it and re-run if Cargo.lock changed."
fi

# ── 4. Build + install ────────────────────────────────────────────────────────

echo ""
echo "Building Flatpak (this will take a while on first run)..."
flatpak-builder \
    --user \
    --install \
    --force-clean \
    "$BUILD_DIR" \
    "$MANIFEST"

echo ""
echo "✓ Installed. Launch with:"
echo "    flatpak run com.amadeuz.Notes"
echo ""
echo "Or find it in your app launcher as 'Amadeuz Notes'."
