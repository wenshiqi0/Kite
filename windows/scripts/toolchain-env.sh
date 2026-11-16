#!/usr/bin/env bash
# Source this from the build entry point; keep user shell configuration untouched.
KITE_WINDOWS_TOOLS_DIR="${KITE_WINDOWS_TOOLS_DIR:-$HOME/.cache/kite-windows-tools}"
if ! command -v cargo >/dev/null && [[ -x "$HOME/.cargo/bin/cargo" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if [[ -x "$KITE_WINDOWS_TOOLS_DIR/root/usr/bin/x86_64-w64-mingw32-gcc" ]]; then
  export PATH="$KITE_WINDOWS_TOOLS_DIR/root/usr/bin:$PATH"
  export LD_LIBRARY_PATH="$KITE_WINDOWS_TOOLS_DIR/root/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
if ! command -v dotnet >/dev/null && [[ -x "$KITE_WINDOWS_TOOLS_DIR/dotnet/dotnet" ]]; then
  export DOTNET_ROOT="$KITE_WINDOWS_TOOLS_DIR/dotnet"
  export PATH="$DOTNET_ROOT:$PATH"
fi
export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1
