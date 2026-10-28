#!/bin/sh
# Builds kite-ffi as a static library for the platform Xcode is building, and
# generates its Swift bindings. Runs as the app target's first build phase;
# run it by hand once before the first `xcodegen generate` (defaults to the
# simulator).
set -eu

IOS_DIR=$(cd "$(dirname "$0")/.." && pwd)
WORKSPACE=$(cd "$IOS_DIR/.." && pwd)
PLATFORM_NAME=${PLATFORM_NAME:-iphonesimulator}
CONFIGURATION=${CONFIGURATION:-Debug}
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

case "$PLATFORM_NAME" in
    iphoneos) TARGET=aarch64-apple-ios ;;
    iphonesimulator) TARGET=aarch64-apple-ios-sim ;;
    *) echo "error: unsupported platform $PLATFORM_NAME" >&2; exit 1 ;;
esac

if [ "$CONFIGURATION" = Release ]; then
    PROFILE=release
    set -- --release
else
    PROFILE=debug
    set --
fi

# Xcode points SDKROOT at the iOS SDK, but cargo also compiles build scripts
# for macOS. cc-rs finds the iOS SDK for the target on its own.
unset SDKROOT
PATH="$HOME/.cargo/bin:$PATH"

cd "$WORKSPACE"
# Cargo.toml declares kite-ffi as a cdylib for Android; iOS links it statically.
cargo rustc -p kite-ffi --lib --crate-type staticlib --target "$TARGET" "$@"

LIB_DIR="$IOS_DIR/build/rust/$PLATFORM_NAME"
mkdir -p "$LIB_DIR"
cp "target/$TARGET/$PROFILE/libkite_ffi.a" "$LIB_DIR/libkite_ffi.a"

# Generate into a scratch directory and swap it in only when the bindings
# changed, so unchanged Rust doesn't make Xcode recompile the Swift side.
GEN_DIR="$IOS_DIR/Generated"
NEXT_DIR="$IOS_DIR/build/generated-next"
rm -rf "$NEXT_DIR"
mkdir -p "$NEXT_DIR/include"
cargo run --quiet -p uniffi-bindgen -- generate \
    --library "$LIB_DIR/libkite_ffi.a" --language swift --no-format --out-dir "$NEXT_DIR"
mv "$NEXT_DIR/kite_ffiFFI.h" "$NEXT_DIR/include/kite_ffiFFI.h"
mv "$NEXT_DIR/kite_ffiFFI.modulemap" "$NEXT_DIR/include/module.modulemap"
if diff -rq "$NEXT_DIR" "$GEN_DIR" >/dev/null 2>&1; then
    rm -rf "$NEXT_DIR"
else
    rm -rf "$GEN_DIR"
    mv "$NEXT_DIR" "$GEN_DIR"
fi
