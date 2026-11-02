#!/bin/sh
set -eu
MAC_DIR=$(cd "$(dirname "$0")/.." && pwd)
python3 - "${1:-$MAC_DIR/build/DerivedData/Build/Products/Debug/Kite.app}" <<'PY'
from pathlib import Path
import datetime, os, plistlib, shutil, subprocess, sys, tempfile
source = Path(sys.argv[1]).resolve()
destination = Path.home() / 'Applications/Kite.app'
if source == destination:
    raise SystemExit('Choose a built application as the install source.')
for line in subprocess.run(['ps', '-axo', 'comm='], capture_output=True, text=True, check=True).stdout.splitlines():
    if line.strip() == str(destination / 'Contents/MacOS/Kite'):
        raise SystemExit('Quit the installed Kite before replacing its application bundle; background host services keep running.')
info = plistlib.loads((source / 'Contents/Info.plist').read_bytes())
if info.get('CFBundleIdentifier') != 'io.github.wenshiqi0.kite.mac':
    raise SystemExit('The source is not the production Kite application.')
def verify(path):
    subprocess.run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(path)], check=True)
verify(source)
destination.parent.mkdir(parents=True, exist_ok=True)
stage = Path(tempfile.mkdtemp(prefix='.Kite-install-', dir=destination.parent))
try:
    candidate = stage / 'Kite.app'
    subprocess.run(['/usr/bin/ditto', str(source), str(candidate)], check=True)
    verify(candidate)
    previous = destination.with_name('.Kite-previous-' + datetime.datetime.now().strftime('%Y%m%d-%H%M%S'))
    if previous.exists():
        raise SystemExit('The private backup path already exists; retry after this second.')
    if destination.exists():
        os.rename(destination, previous)
    try:
        os.rename(candidate, destination)
        verify(destination)
    except Exception:
        if destination.exists():
            os.rename(destination, candidate)
        if previous.exists():
            os.rename(previous, destination)
        raise
finally:
    shutil.rmtree(stage)
print(destination)
PY
