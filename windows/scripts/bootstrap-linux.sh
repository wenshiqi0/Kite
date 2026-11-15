#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/toolchain-env.sh"
mkdir -p "$KITE_WINDOWS_TOOLS_DIR"
if ! command -v x86_64-w64-mingw32-gcc >/dev/null || ! command -v cmake >/dev/null; then
  command -v apt-get >/dev/null || { echo 'Install MinGW-w64 (POSIX threads), CMake and Ninja first.' >&2; exit 1; }
  # Rootless Debian 13 bootstrap: apt verifies packages using its signed index.
  mkdir -p "$KITE_WINDOWS_TOOLS_DIR/packages" "$KITE_WINDOWS_TOOLS_DIR/root"
  (
    cd "$KITE_WINDOWS_TOOLS_DIR/packages"
    apt-get download gcc-mingw-w64-x86-64-posix g++-mingw-w64-x86-64-posix \
      gcc-mingw-w64-x86-64-posix-runtime gcc-mingw-w64-base binutils-mingw-w64-x86-64 \
      mingw-w64-x86-64-dev mingw-w64-common cmake cmake-data librhash1 libjsoncpp26 ninja-build
    for package in ./*.deb; do dpkg-deb -x "$package" "$KITE_WINDOWS_TOOLS_DIR/root"; done
    ln -sf x86_64-w64-mingw32-gcc-posix "$KITE_WINDOWS_TOOLS_DIR/root/usr/bin/x86_64-w64-mingw32-gcc"
    ln -sf x86_64-w64-mingw32-g++-posix "$KITE_WINDOWS_TOOLS_DIR/root/usr/bin/x86_64-w64-mingw32-g++"
  )
fi
if ! command -v dotnet >/dev/null; then
  # Fetch the pinned SDK from Microsoft's release metadata and verify its SHA-512.
  python3 - "$KITE_WINDOWS_TOOLS_DIR" <<'PY'
import hashlib,json,pathlib,sys,tarfile,urllib.request
root=pathlib.Path(sys.argv[1])
with urllib.request.urlopen('https://builds.dotnet.microsoft.com/dotnet/release-metadata/10.0/releases.json') as response:
    releases=json.load(response)
sdks=[sdk for release in releases['releases'] for sdk in release.get('sdks',[release['sdk']])]
sdk=next(sdk for sdk in sdks if sdk['version']=='10.0.404')
artifact=next(file for file in sdk['files'] if file['rid']=='linux-x64' and file['name'].endswith('.tar.gz'))
archive=root/'dotnet-sdk.tar.gz'
urllib.request.urlretrieve(artifact['url'],archive)
if hashlib.sha512(archive.read_bytes()).hexdigest()!=artifact['hash'].lower():
    raise SystemExit('Microsoft SDK checksum mismatch')
(root/'dotnet').mkdir(exist_ok=True)
with tarfile.open(archive) as bundle: bundle.extractall(root/'dotnet',filter='data')
archive.unlink()
PY
fi
source "$(dirname "${BASH_SOURCE[0]}")/toolchain-env.sh"
rustup target add x86_64-pc-windows-gnu
x86_64-w64-mingw32-gcc --version | head -1
cmake --version | head -1
dotnet --version
