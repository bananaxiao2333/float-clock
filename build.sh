#!/usr/bin/env bash
# Cross-compile the executables for all three platforms, plus the macOS .app,
# and pack every one of them into a .zip.
#
#   ./build.sh                 # all three platforms
#   ./build.sh macos           # macOS only
#   ./build.sh linux windows
#
# Everything lands in dist/.
#
# Why .zip and nothing else: a browser download strips the executable bit, and
# Finder then treats a bare Mach-O binary as a text file and hands it to
# TextEdit - which greets the user with "the text encoding Unicode (UTF-8) is
# not applicable". A zip (like a tar) records the mode, so unpacking restores it.
# Shipping one archive format for every platform also means there is no "which
# file do I download" question.
#
# The icons must have been generated first: python3 tools/make_icons.py
set -euo pipefail

cd "$(dirname "$0")"

# cargo / rustup live in ~/.cargo/bin, which is not necessarily on PATH
export PATH="$HOME/.cargo/bin:$PATH"

DIST="dist"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
ROOT="$(pwd)"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

rm -rf "$DIST"
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
            *) echo "unknown platform: $name (expected macos, linux or windows)" >&2; exit 2 ;;
        esac
    done
fi

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing $1. $2" >&2
        exit 1
    fi
}

have_target() {
    # rustup's bookkeeping and the files on disk occasionally disagree (an
    # interrupted install, usually), so check both. Either one saying "yes"
    # counts, which avoids a pointless reinstall that then hits a conflict.
    rustup target list --installed | grep -qx "$1" && return 0
    local libdir
    libdir="$(rustc --print target-libdir --target "$1" 2>/dev/null)" || return 1
    [ -d "$libdir" ]
}

require_target() {
    if ! have_target "$1"; then
        echo "installing target $1 ..."
        rustup target add "$1"
    fi
}

# Drop the shared reading material into a staging folder and zip it up.
# Usage: pack <zip-name> <staged-dir>
pack() {
    local zip_name="$1" stage="$2"
    cp QUICKSTART.txt config.example.toml "$stage/"
    (cd "$stage" && zip -qry "$ROOT/$DIST/$zip_name" .)
    echo "  -> $DIST/$zip_name"
}

echo "== unit tests =="
cargo test --quiet

if [ "$WANT_MACOS" = 1 ]; then
    echo
    echo "== macOS (universal binary + .app) =="
    require_target aarch64-apple-darwin
    require_target x86_64-apple-darwin
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin

    MACOS_STAGE="$STAGE/macos"
    mkdir -p "$MACOS_STAGE"
    if command -v lipo >/dev/null 2>&1; then
        lipo -create \
            target/aarch64-apple-darwin/release/float-clock \
            target/x86_64-apple-darwin/release/float-clock \
            -output "$MACOS_STAGE/float-clock"
        APP_SOURCE="$MACOS_STAGE/float-clock"
        echo "  built an arm64 + x86_64 universal binary"
    else
        cp target/aarch64-apple-darwin/release/float-clock "$MACOS_STAGE/float-clock-arm64"
        cp target/x86_64-apple-darwin/release/float-clock "$MACOS_STAGE/float-clock-x86_64"
        APP_SOURCE="$MACOS_STAGE/float-clock-arm64"
        echo "  no lipo found, shipping the two slices separately" >&2
    fi

    # The .app is what "download and double-click" actually needs: it carries the
    # icon, and unpacking the zip restores the executable bit on the binary
    # inside. The bundle is called FloatClock.app because Finder and the Dock
    # show the file name, not CFBundleName.
    tools/make_app.sh "$APP_SOURCE" "$VERSION" "$MACOS_STAGE"
    rm -f "$MACOS_STAGE/float-clock" "$MACOS_STAGE/float-clock-arm64" \
          "$MACOS_STAGE/float-clock-x86_64"
    pack float-clock-macos-universal.zip "$MACOS_STAGE"
fi

if [ "$WANT_LINUX" = 1 ]; then
    echo
    echo "== Linux x86_64 =="
    # zig brings its own glibc and linker, so no VM or container is needed
    need zig "brew install zig"
    need cargo-zigbuild "cargo install cargo-zigbuild"
    require_target x86_64-unknown-linux-gnu
    # 2.28 is the floor on most distributions, which makes it the widest target
    cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.28

    LINUX_STAGE="$STAGE/linux"
    mkdir -p "$LINUX_STAGE"
    install -m 0755 target/x86_64-unknown-linux-gnu/release/float-clock \
        "$LINUX_STAGE/float-clock"
    pack float-clock-linux-x86_64.zip "$LINUX_STAGE"
fi

if [ "$WANT_WINDOWS" = 1 ]; then
    echo
    echo "== Windows x86_64 =="
    require_target x86_64-pc-windows-gnu
    if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
        echo "missing the mingw-w64 linker. brew install mingw-w64" >&2
        exit 1
    fi
    cargo build --release --target x86_64-pc-windows-gnu

    WINDOWS_STAGE="$STAGE/windows"
    mkdir -p "$WINDOWS_STAGE"
    cp target/x86_64-pc-windows-gnu/release/float-clock.exe \
       "$WINDOWS_STAGE/float-clock.exe"
    pack float-clock-windows-x86_64.zip "$WINDOWS_STAGE"
fi

echo
echo "== checksums =="
(
    cd "$DIST"
    rm -f SHA256SUMS
    # Only the published artefacts: the raw binaries stay out of dist/ entirely.
    shasum -a 256 ./*.zip | sed 's|\./||' | sort -k 2 > SHA256SUMS
    cat SHA256SUMS
)

echo
echo "== artefacts =="
ls -lh "$DIST"
