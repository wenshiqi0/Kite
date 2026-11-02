#!/bin/sh
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
CONFIG="$HOME/Library/Application Support/io.github.wenshiqi0.kite.mac/build/signing-identity"
IDENTITY=${KITE_MAC_SIGNING_IDENTITY:-$(cat "$CONFIG")}
case "$IDENTITY" in *[!0-9A-Fa-f]*|'') echo 'error: configure a persistent signing certificate first' >&2; exit 1;; esac
test "${#IDENTITY}" -eq 40
ROOT="$HOME/Library/Application Support/io.github.wenshiqi0.kite.mac/native"
TARGET="$ROOT/KiteKeychainHelper-v1"
mkdir -p "$ROOT"
chmod 700 "$ROOT"
if [ ! -e "$TARGET" ]; then
    NEXT=$(mktemp -d "$ROOT/.helper-build.XXXXXX")
    trap 'rm -rf "$NEXT"' EXIT HUP INT TERM
    printf 'let kiteCallerRequirement = "identifier \\"io.github.wenshiqi0.kite.mac\\" and certificate leaf = H\\"%s\\""\n' "$IDENTITY" > "$NEXT/Configuration.swift"
    swiftc -O -parse-as-library "$MAC_DIR/scripts/keychain-helper.swift" "$NEXT/Configuration.swift" -o "$NEXT/helper"
    codesign --force --sign "$IDENTITY" --timestamp=none --options runtime --identifier io.github.wenshiqi0.kite.mac.keychain-helper "$NEXT/helper"
    chmod 500 "$NEXT/helper"
    # Atomic no-overwrite install: ordinary app builds never replace this file.
    ln "$NEXT/helper" "$TARGET"
    rm -rf "$NEXT"
    trap - EXIT HUP INT TERM
fi
codesign --verify --strict -R="identifier \"io.github.wenshiqi0.kite.mac.keychain-helper\" and certificate leaf = H\"$IDENTITY\"" "$TARGET"
exec "$TARGET" --authorize
