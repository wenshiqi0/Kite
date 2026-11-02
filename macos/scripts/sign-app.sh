#!/bin/sh
set -eu
APP=${1:?usage: sign-app.sh /path/to/Kite.app}
IDENTITY=${KITE_MAC_SIGNING_IDENTITY:-}
if [ "${KITE_MAC_ADHOC:-0}" = 1 ]; then
    IDENTITY=-
elif [ -z "$IDENTITY" ]; then
    CONFIG="$HOME/Library/Application Support/io.github.wenshiqi0.kite.mac/build/signing-identity"
    if [ -f "$CONFIG" ]; then IDENTITY=$(cat "$CONFIG"); fi
fi
if [ -z "$IDENTITY" ]; then
    echo 'error: configure a persistent signing identity with macos/scripts/configure-signing.sh; CI may explicitly set KITE_MAC_ADHOC=1' >&2
    exit 1
fi
if [ "$IDENTITY" != - ]; then
    case "$IDENTITY" in *[!0-9A-Fa-f]*) echo 'error: use a pinned certificate SHA-1' >&2; exit 1;; esac
    test "${#IDENTITY}" -eq 40
fi
for LIBRARY in "$APP"/Contents/MacOS/*.dylib; do
    if [ -f "$LIBRARY" ]; then codesign --force --sign "$IDENTITY" --timestamp=none "$LIBRARY"; fi
done
codesign --force --sign "$IDENTITY" --timestamp=none --identifier io.github.wenshiqi0.kite.mac "$APP"
codesign --verify --deep --strict "$APP"
