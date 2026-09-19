#!/usr/bin/env bash
# 一条命令交叉编译出三平台单文件可执行程序。
#
#   ./build.sh                 # 三平台全出
#   ./build.sh macos           # 只出 macOS（通用二进制）
#   ./build.sh linux windows
#
# 产物都在 dist/ 下。
set -euo pipefail

cd "$(dirname "$0")"

# cargo / rustup 装在 ~/.cargo/bin，不保证已经在 PATH 里
export PATH="$HOME/.cargo/bin:$PATH"

DIST="dist"
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
    echo "== macOS（通用二进制）=="
    require_target aarch64-apple-darwin
    require_target x86_64-apple-darwin
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    if command -v lipo >/dev/null 2>&1; then
        lipo -create \
            target/aarch64-apple-darwin/release/float-clock \
            target/x86_64-apple-darwin/release/float-clock \
            -output "$DIST/float-clock-macos-universal"
        echo "  → $DIST/float-clock-macos-universal（arm64 + x86_64）"
    else
        cp target/aarch64-apple-darwin/release/float-clock "$DIST/float-clock-macos-arm64"
        cp target/x86_64-apple-darwin/release/float-clock "$DIST/float-clock-macos-x86_64"
        echo "  → $DIST/float-clock-macos-{arm64,x86_64}（没找到 lipo，分开出）"
    fi
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
    cp target/x86_64-unknown-linux-gnu/release/float-clock "$DIST/float-clock-linux-x86_64"
    echo "  → $DIST/float-clock-linux-x86_64"
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
    echo "  → $DIST/float-clock-windows-x86_64.exe"
fi

echo
echo "== 产物 =="
ls -lh "$DIST"
