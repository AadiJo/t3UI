#!/usr/bin/env bash
# Builds dist/T3UI.app and dist/T3UI-<version>-<arch>.dmg from a release binary.
# Usage: script/bundle-macos.sh [path/to/t3ui]   (defaults to target/release/t3ui)
set -euo pipefail

cd "$(dirname "$0")/.."
BIN="${1:-target/release/t3ui}"
VERSION="${T3UI_VERSION:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}"
ARCH="$(uname -m)"
APP="dist/T3UI.app"

rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/t3ui"
cp assets/app/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>T3UI</string>
  <key>CFBundleDisplayName</key><string>T3UI</string>
  <key>CFBundleIdentifier</key><string>com.aadijo.t3ui</string>
  <key>CFBundleExecutable</key><string>t3ui</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>com.aadijo.t3ui</string>
      <key>CFBundleURLSchemes</key><array><string>t3ui</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

# Ad-hoc signature: no Developer ID is available, but arm64 needs a valid signature to launch.
codesign --force --deep --sign - "$APP"

DMG="dist/T3UI-${VERSION}-${ARCH}.dmg"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "T3UI" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "$DMG"
