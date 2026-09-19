#!/usr/bin/env bash
# 把 macOS 的可执行文件包成一个 .app。
#
# 为什么非包不可：GitHub Releases 下载下来的是「裸二进制」，可执行位会丢，
# Finder 于是把它当成文本文件丢给「文本编辑」，报一句
#   「无法打开文件，文字编码 Unicode (UTF-8) 不适用」
# 打成 .app 再 zip 起来就不会有这个问题：解压后双击就能开，图标也在。
#
# 用法:
#   tools/make_app.sh <可执行文件> <版本号> <输出目录>
#
# 产物: <输出目录>/FloatClock.app
set -euo pipefail

BIN="${1:?用法: tools/make_app.sh <可执行文件> <版本号> <输出目录>}"
VERSION="${2:?缺少版本号}"
OUT_DIR="${3:?缺少输出目录}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# 允许传相对路径，也允许只传一个文件名
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
    <!-- 有窗口但不占 Dock 图标，本来在代码里设了 Accessory，这里双保险 -->
    <key>LSUIElement</key>                  <true/>
</dict>
</plist>
PLIST

printf 'APPL????' > "$APP/Contents/PkgInfo"

# 顺手做一次 ad-hoc 签名：没做的话 Apple Silicon 上可能因为「签名无效」直接被杀
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
fi

echo "  → $APP"
