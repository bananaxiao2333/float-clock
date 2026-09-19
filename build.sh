#!/usr/bin/env bash
# 一条命令交叉编译出三平台可执行程序（外加 macOS 的 .app 和两个压缩包）。
#
#   ./build.sh                 # 三平台全出
#   ./build.sh macos           # 只出 macOS
#   ./build.sh linux windows
#
# 产物都在 dist/ 下。图标需要先在 macOS 上跑过 tools/make_icons.py。
set -euo pipefail

cd "$(dirname "$0")"

# cargo / rustup 装在 ~/.cargo/bin，不保证已经在 PATH 里
export PATH="$HOME/.cargo/bin:$PATH"

DIST="dist"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p "$DIST"

WANT_MACOS=0
WANT_LINUX=0
WANT_WINDOWS=0
if [ $# -eq 0 ]; then
    WANT_MACOS=1; WANT_LINUX=1; WANT_WINDOWS=1
else
    for name in "$@"; do
        case "$name" in
            macos|mac)     WANT_MACOS=1 ;;
            linux|lin)     WANT_LINUX=1 ;;
            windows|win)   WANT_WINDOWS=1 ;;
            *) echo "不认识的平台：$name（可选 macos / linux / windows）" >&2; exit 2 ;;
        esac
    done
fi

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "缺少 $1。$2" >&2
        exit 1
    fi
}

have_target() {
    # rustup 的记录偶尔会和磁盘上的实际状态对不上（比如上次安装中途被打断），
    # 两个地方都看一眼：只要有一边说「有」就当它有了，免得白装一遍再撞冲突。
    rustup target list --installed | grep -qx "$1" && return 0
    local libdir
    libdir="$(rustc --print target-libdir --target "$1" 2>/dev/null)" || return 1
    [ -d "$libdir" ]
}

require_target() {
    if ! have_target "$1"; then
        echo "还没装目标 $1，正在安装…"
        rustup target add "$1"
    fi
}

echo "== 单元测试 =="
cargo test --quiet

if [ "$WANT_MACOS" = 1 ]; then
    echo
    echo "== macOS（通用二进制 + .app）=="
    require_target aarch64-apple-darwin
    require_target x86_64-apple-darwin
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin

    rm -rf "$DIST/FloatClock.app"
    if command -v lipo >/dev/null 2>&1; then
        lipo -create \
            target/aarch64-apple-darwin/release/float-clock \
            target/x86_64-apple-darwin/release/float-clock \
            -output "$DIST/float-clock-macos-universal"
        chmod +x "$DIST/float-clock-macos-universal"
        echo "  → $DIST/float-clock-macos-universal（arm64 + x86_64）"
        APP_SOURCE="$DIST/float-clock-macos-universal"
    else
        cp target/aarch64-apple-darwin/release/float-clock "$DIST/float-clock-macos-arm64"
        cp target/x86_64-apple-darwin/release/float-clock "$DIST/float-clock-macos-x86_64"
        APP_SOURCE="$DIST/float-clock-macos-arm64"
        echo "  → $DIST/float-clock-macos-{arm64,x86_64}（没找到 lipo，分开出）"
    fi

    # .app 才是给「下载下来双击」用的：裸二进制过一道浏览器下载会丢掉可执行位，
    # Finder 会把它当文本文件丢给「文本编辑」。
    # 包名保持 FloatClock.app —— Finder / Dock 显示的是文件名，不是 CFBundleName。
    tools/make_app.sh "$APP_SOURCE" "$VERSION" "$DIST"
    rm -f "$DIST/float-clock-macos-universal.zip"
    (cd "$DIST" && zip -qry float-clock-macos-universal.zip FloatClock.app)
    echo "  → $DIST/float-clock-macos-universal.zip（解压双击就能开）"
fi

if [ "$WANT_LINUX" = 1 ]; then
    echo
    echo "== Linux x86_64 =="
    # 用 zig 自带的一套 glibc + 链接器，不需要虚拟机或容器
    need zig "brew install zig"
    need cargo-zigbuild "cargo install cargo-zigbuild"
    require_target x86_64-unknown-linux-gnu
    # 2.28 是很多发行版的底线，用它当目标 glibc 版本兼容面最宽
    cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.28
    install -m 0755 target/x86_64-unknown-linux-gnu/release/float-clock \
        "$DIST/float-clock-linux-x86_64"
    echo "  → $DIST/float-clock-linux-x86_64"

    # tar.gz 能保住可执行位（浏览器下载 + 解压后直接就能跑）
    rm -rf "$DIST/float-clock-linux-x86_64.tar.gz"
    tar -czf "$DIST/float-clock-linux-x86_64.tar.gz" \
        -C "$DIST" float-clock-linux-x86_64
    echo "  → $DIST/float-clock-linux-x86_64.tar.gz"
fi

if [ "$WANT_WINDOWS" = 1 ]; then
    echo
    echo "== Windows x86_64 =="
    require_target x86_64-pc-windows-gnu
    if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
        echo "缺少 mingw-w64 链接器。brew install mingw-w64" >&2
        exit 1
    fi
    cargo build --release --target x86_64-pc-windows-gnu
    cp target/x86_64-pc-windows-gnu/release/float-clock.exe \
       "$DIST/float-clock-windows-x86_64.exe"
    echo "  → $DIST/float-clock-windows-x86_64.exe（图标和版本信息已写进 exe）"
fi

echo
echo "== 校验和 =="
(
    cd "$DIST"
    rm -f SHA256SUMS
    find . -maxdepth 1 -type f ! -name SHA256SUMS -exec shasum -a 256 {} + \
        | sed 's|\./||' | sort -k 2 > SHA256SUMS
    cat SHA256SUMS
)

echo
echo "== 产物 =="
ls -lh "$DIST"
