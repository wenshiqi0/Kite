#!/bin/sh
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
KITE_ARCH=${CURRENT_ARCH:-$(uname -m)}
if [ "$KITE_ARCH" = undefined_arch ]; then KITE_ARCH=${NATIVE_ARCH_ACTUAL:-$(uname -m)}; fi
HOST_DIR="$MAC_DIR/build/host/$KITE_ARCH/${CONFIGURATION:-Debug}"
DESTINATION="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/host"
HOST_APP="$DESTINATION/Kite Host.app"
mkdir -p "$HOST_APP/Contents/MacOS"
cp -f "$HOST_DIR/kite-node" "$HOST_DIR/bun" "$HOST_DIR/host-platform" "$HOST_APP/Contents/MacOS/"
cp -f "$MAC_DIR/HostPlatform/Info.plist" "$HOST_APP/Contents/Info.plist"
if [ -f "$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/AppIcon.icns" ]; then
    mkdir -p "$HOST_APP/Contents/Resources"
    cp -f "$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/AppIcon.icns" "$HOST_APP/Contents/Resources/AppIcon.icns"
fi
chmod 755 "$HOST_APP/Contents/MacOS/"*
# Remove only the preceding flat generated layout, never user runtime data.
rm -f "$DESTINATION/kite-node" "$DESTINATION/bun" "$DESTINATION/host-platform"
