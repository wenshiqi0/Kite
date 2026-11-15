#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(cd "$script_dir/../.." && pwd)"
source "$script_dir/toolchain-env.sh"
cd "$repo_dir"
for tool in cargo rustup bun dotnet cmake ninja x86_64-w64-mingw32-gcc x86_64-w64-mingw32-g++ python3; do
  command -v "$tool" >/dev/null || { echo "Missing $tool. Run windows/scripts/bootstrap-linux.sh; install Rust and Bun 1.3.14+ if needed." >&2; exit 1; }
done
if [[ -n "$(git submodule status --recursive | sed -n '/^[-+U]/p')" ]]; then
  echo 'Initialize pinned submodules with git submodule update --init --recursive.' >&2; exit 1
fi
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
rustup target add x86_64-pc-windows-gnu
bun install --frozen-lockfile
dotnet run --project windows/Tests/Kite.LogicTests.csproj -c Release
cargo build --locked --release --target x86_64-pc-windows-gnu -p kite-ffi --features windows-client
cmake -S windows/native -B windows/build/native -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_TOOLCHAIN_FILE="$repo_dir/windows/native/mingw.cmake"
cmake --build windows/build/native --parallel "$CARGO_BUILD_JOBS"
package_dir="$repo_dir/windows/dist/Kite-windows-x64"
mkdir -p "$package_dir"
dotnet publish windows/Kite/Kite.csproj -c Release -r win-x64 --self-contained true \
  -p:PublishReadyToRun=false -p:UseAppHost=true -p:TreatWarningsAsErrors=true -o "$package_dir"
cp "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-gnu/release/kite_ffi.dll" "$package_dir/"
cp windows/build/native/kite_windows_media.dll "$package_dir/"
cp LICENSE NOTICE.txt "$package_dir/"
cp windows/build/native/_deps/opus-src/COPYING "$package_dir/OPUS-LICENSE.txt"
cp windows/QUICKSTART.txt "$package_dir/README.txt"
python3 windows/scripts/package.py "$package_dir"
