#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Amadeuz"
BUNDLE_ID="com.amadeuz.notes"
VERSION="0.1.0"
TARGET="Notes"          # must match the SPM target name in Package.swift
BUILD_DIR=".build/release"
OUT_DIR="dist"
APP_BUNDLE="$OUT_DIR/$APP_NAME.app"

echo "→ Building $APP_NAME..."
swift build -c release

echo "→ Assembling $APP_BUNDLE..."
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

cp "$BUILD_DIR/$TARGET" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"

cat > "$APP_BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
</dict>
</plist>
PLIST

echo "→ Code-signing (ad-hoc)..."
codesign --force --deep --sign - "$APP_BUNDLE"

echo ""
echo "✓ Built: $APP_BUNDLE"
echo ""
echo "  To install: cp -r $APP_BUNDLE /Applications/"
echo "  Or drag $APP_BUNDLE to your Applications folder."
