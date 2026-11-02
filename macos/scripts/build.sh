#!/bin/sh
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
"$MAC_DIR/scripts/build-rust.sh"
xcodegen generate --spec "$MAC_DIR/project.yml"
xcodebuild -project "$MAC_DIR/Kite.xcodeproj" -scheme Kite -configuration Debug \
    -derivedDataPath "$MAC_DIR/build/DerivedData" CODE_SIGNING_ALLOWED=NO build "$@"
"$MAC_DIR/scripts/sign-app.sh" "$MAC_DIR/build/DerivedData/Build/Products/Debug/Kite.app"
