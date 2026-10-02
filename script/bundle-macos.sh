#!/usr/bin/env bash
# Builds "dist/T3 Code (GPUI).app" and dist/T3-Code-GPUI-<version>-<arch>.dmg from a release binary.
# Usage: script/bundle-macos.sh [path/to/t3ui]   (defaults to target/release/t3ui)
# Needs macOS with Xcode 26+ (actool compiles the Icon Composer icon). Set T3UI_ALLOW_ICNS_ONLY=1
# to fall back to the .icns icon when no such Xcode is available.
set -euo pipefail

cd "$(dirname "$0")/.."
BIN="${1:-target/release/t3ui}"
VERSION="${T3UI_VERSION:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}"
ARCH="$(uname -m)"
NAME="T3 Code (GPUI)"
APP="dist/$NAME.app"
RES="$APP/Contents/Resources"

rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$RES"
cp "$BIN" "$APP/Contents/MacOS/t3ui"

# Icon, mirroring the reference packager (build-desktop-artifact.ts: generateMacIconSet + actool).
TMP="$(mktemp -d)"
ICONSET="$TMP/icon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/app/black-macos-1024.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) assets/app/black-macos-1024.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$RES/AppIcon.icns"

# actool needs Xcode 26+ for .icon files; pick the newest installed Xcode.
XCODE="$(ls -d /Applications/Xcode*.app 2>/dev/null | sort -V | tail -1 || true)"
ICON_NAME_KEY=""
if [ -n "$XCODE" ] && DEVELOPER_DIR="$XCODE/Contents/Developer" xcrun actool assets/app/t3-code.icon \
    --compile "$TMP" --output-format human-readable-text --notices --warnings \
    --output-partial-info-plist "$TMP/partial.plist" --app-icon t3-code --include-all-app-icons \
    --enable-on-demand-resources NO --development-region en --target-device mac \
    --minimum-deployment-target 26.0 --platform macosx >"$TMP/actool.log" 2>&1 \
    && [ -f "$TMP/Assets.car" ]; then
  cp "$TMP/Assets.car" "$RES/Assets.car"
  ICON_NAME_KEY="<key>CFBundleIconName</key><string>t3-code</string>"
  echo "icon: Assets.car compiled with $XCODE"
elif [ "${T3UI_ALLOW_ICNS_ONLY:-0}" = "1" ]; then
  echo "warning: actool failed; shipping .icns only" >&2
  cat "$TMP/actool.log" >&2 || true
else
  echo "error: actool could not compile assets/app/t3-code.icon (needs Xcode 26+)" >&2
  cat "$TMP/actool.log" >&2 || true
  exit 1
fi
rm -rf "$TMP"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>${NAME}</string>
  <key>CFBundleDisplayName</key><string>${NAME}</string>
  <key>CFBundleIdentifier</key><string>com.aadijo.t3ui</string>
  <key>CFBundleExecutable</key><string>t3ui</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  ${ICON_NAME_KEY}
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

DMG="dist/T3-Code-GPUI-${VERSION}-${ARCH}.dmg"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "$DMG"
