#!/bin/sh
# Pin a local code-signing identity once; the public certificate hash is not a secret.
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
IDENTITY=${1:-}
case "$IDENTITY" in
    *[!0-9A-Fa-f]*|'') echo 'usage: configure-signing.sh <40-character certificate SHA-1>' >&2; exit 1 ;;
esac
test "${#IDENTITY}" -eq 40
security find-identity -v -p codesigning | /usr/bin/grep -Fqi "$IDENTITY" || {
    echo 'error: the requested code-signing identity is unavailable' >&2; exit 1;
}
CONFIG_DIR="$HOME/Library/Application Support/io.github.wenshiqi0.kite.mac/build"
mkdir -p "$CONFIG_DIR"
chmod 700 "$CONFIG_DIR"
umask 077
printf '%s\n' "$IDENTITY" > "$CONFIG_DIR/signing-identity"
mkdir -p "$MAC_DIR/build"
printf 'KITE_MAC_SIGNING_IDENTITY = %s\n' "$IDENTITY" > "$MAC_DIR/build/local-signing.xcconfig"
echo 'Pinned the local Kite signing identity.'
