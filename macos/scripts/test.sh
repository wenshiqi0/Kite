#!/bin/sh
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
"$MAC_DIR/scripts/build-rust.sh"
xcodegen generate --spec "$MAC_DIR/project.yml"
xcodebuild -project "$MAC_DIR/Kite.xcodeproj" -scheme Kite -configuration Debug \
    -derivedDataPath "$MAC_DIR/build/TestDerivedData" -destination "platform=macOS,arch=$(uname -m)" \
    KITE_APP_BUNDLE_IDENTIFIER=io.github.wenshiqi0.kite.mac.test-host CODE_SIGNING_ALLOWED=NO test "$@"
