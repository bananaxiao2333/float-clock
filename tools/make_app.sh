#!/usr/bin/env bash
# Wrap a macOS executable into a .app bundle.
#
# Why a bundle is not optional: what a browser downloads from GitHub Releases is
# a bare binary, and the executable bit does not survive the round trip. Finder
# then treats a Mach-O file as text and hands it to TextEdit, which reports
#   "the file could not be opened. The text encoding Unicode (UTF-8) is not
#    applicable."
# Inside a .app, inside a zip, none of that happens: unpack, double-click, done,
# and the icon is there too.
#
# Usage:
#   tools/make_app.sh <executable> <version> <output-dir>
#
# Produces: <output-dir>/FloatClock.app
set -euo pipefail

BIN="${1:?usage: tools/make_app.sh <executable> <version> <output-dir>}"
VERSION="${2:?missing version}"
OUT_DIR="${3:?missing output directory}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Accept a relative path, or just a bare file name
BIN_DIR="$(dirname "$BIN")"
BIN_PATH="$(cd "$BIN_DIR" && pwd)/$(basename "$BIN")"
APP="$OUT_DIR/FloatClock.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN_PATH" "$APP/Contents/MacOS/float-clock"
chmod +x "$APP/Contents/MacOS/float-clock"

if [ -f "$ROOT/assets/icon.icns" ]; then
    cp "$ROOT/assets/icon.icns" "$APP/Contents/Resources/FloatClock.icns"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                 <string>FloatClock</string>
    <key>CFBundleDisplayName</key>          <string>FloatClock</string>
    <key>CFBundleIdentifier</key>           <string>com.bananaxiao2333.float-clock</string>
    <key>CFBundleExecutable</key>           <string>float-clock</string>
    <key>CFBundleIconFile</key>             <string>FloatClock</string>
    <key>CFBundlePackageType</key>          <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleShortVersionString</key>   <string>${VERSION}</string>
    <key>CFBundleVersion</key>              <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>       <string>10.15</string>
    <key>NSHighResolutionCapable</key>      <true/>
    <!-- May own windows but takes no Dock icon; the code sets the
         Accessory activation policy too, this is belt and braces -->
    <key>LSUIElement</key>                  <true/>
</dict>
</plist>
PLIST

printf 'APPL????' > "$APP/Contents/PkgInfo"

# Ad-hoc sign it while we are here: without any signature at all,
# Apple Silicon may kill the process for having an invalid one
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
fi

echo "  → $APP"
