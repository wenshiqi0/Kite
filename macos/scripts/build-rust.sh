#!/bin/sh
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$MAC_DIR/build"
if [ "${KITE_MAC_RUST_BUILD_LOCKED:-0}" != 1 ]; then
    # OpenSSL already requires Perl. Keep its parent alive to hold the lock
    # through Cargo, static-library copying and the generated-binding swap.
    exec /usr/bin/perl -MFcntl=:flock -e '
        open(my $lock, ">>", shift @ARGV) or die $!;
        flock($lock, LOCK_EX) or die $!;
        $ENV{KITE_MAC_RUST_BUILD_LOCKED} = 1;
        system @ARGV;
        exit($? == -1 ? 1 : $? & 127 ? 128 + ($? & 127) : $? >> 8);
    ' "$MAC_DIR/build/rust-build.lock" "$0" "$@"
fi
WORKSPACE=$(cd "$MAC_DIR/.." && pwd)
CONFIGURATION=${CONFIGURATION:-Debug}
KITE_ARCH=${CURRENT_ARCH:-$(uname -m)}
if [ "$KITE_ARCH" = undefined_arch ]; then KITE_ARCH=${NATIVE_ARCH_ACTUAL:-$(uname -m)}; fi
case "$KITE_ARCH" in
    arm64) TARGET=aarch64-apple-darwin ;;
    x86_64) TARGET=x86_64-apple-darwin ;;
    *) echo "error: unsupported Mac architecture $KITE_ARCH" >&2; exit 1 ;;
esac
if [ "$CONFIGURATION" = Release ]; then PROFILE=release; set -- --release; else PROFILE=debug; set --; fi
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
# Both build scripts and the library target macOS. Xcode's direct clang needs
# an explicit SDK (the /usr/bin compiler shim otherwise hides this difference).
export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
export CC="$(xcrun --sdk macosx --find clang)"
export AR="$(xcrun --sdk macosx --find ar)"
PATH="$HOME/.cargo/bin:$PATH"
cd "$WORKSPACE"
cargo rustc -p kite-ffi --lib --crate-type staticlib --target "$TARGET" "$@"
LIB_DIR="$MAC_DIR/build/rust/$KITE_ARCH/$CONFIGURATION"
mkdir -p "$LIB_DIR"
cp "${CARGO_TARGET_DIR:-target}/$TARGET/$PROFILE/libkite_ffi.a" "$LIB_DIR/libkite_ffi.a"
NEXT_DIR="$MAC_DIR/build/generated-next-$KITE_ARCH"
rm -rf "$NEXT_DIR"
mkdir -p "$NEXT_DIR/include"
cargo run --quiet -p uniffi-bindgen -- generate --library "$LIB_DIR/libkite_ffi.a" --language swift --no-format --out-dir "$NEXT_DIR"
mv "$NEXT_DIR/kite_ffiFFI.h" "$NEXT_DIR/include/kite_ffiFFI.h"
mv "$NEXT_DIR/kite_ffiFFI.modulemap" "$NEXT_DIR/include/module.modulemap"
if diff -rq "$NEXT_DIR" "$MAC_DIR/Generated" >/dev/null 2>&1; then
    rm -rf "$NEXT_DIR"
else
    rm -rf "$MAC_DIR/Generated"
    mv "$NEXT_DIR" "$MAC_DIR/Generated"
fi
cargo build -p kite-node --target "$TARGET" "$@"
HOST_DIR="$MAC_DIR/build/host/$KITE_ARCH/$CONFIGURATION"
mkdir -p "$HOST_DIR"
cp "${CARGO_TARGET_DIR:-target}/$TARGET/$PROFILE/kite-node" "$HOST_DIR/kite-node"
cp -f "$(command -v bun)" "$HOST_DIR/bun"
chmod 755 "$HOST_DIR/bun"
xcrun swiftc -O -target "$KITE_ARCH-apple-macosx14.0" -sdk "$SDKROOT" \
    "$MAC_DIR/HostPlatform/main.swift" -o "$HOST_DIR/host-platform"
