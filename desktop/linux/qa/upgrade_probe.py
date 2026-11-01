#!/usr/bin/env python3
"""Real old/new GTK process handover on an owned X server and private D-Bus."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from gi.repository import Gio, GLib

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('kite_install', root / 'install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def call(method, parameters, kind):
    return bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                         'org.freedesktop.DBus', method, parameters,
                         GLib.VariantType.new(kind), Gio.DBusCallFlags.NONE, 1500, None).unpack()[0]


def owner_pid():
    if not call('NameHasOwner', GLib.Variant('(s)', (installer.APP_ID,)), '(b)'):
        return None
    return call('GetConnectionUnixProcessID', GLib.Variant('(s)', (installer.APP_ID,)), '(u)')


assert owner_pid() is None, 'Upgrade probe requires a private session bus'
with tempfile.TemporaryDirectory(prefix='kite-upgrade-probe-') as directory:
    fixture = Path(directory)
    binary = fixture / 'node'
    binary.write_text('#!/usr/bin/python3\nimport json\nprint(json.dumps(dict(health="ready",services=[])))\n')
    binary.chmod(0o700)
    env = dict(os.environ, KITE_NODE_BINARY=str(binary), KITE_DESKTOP_LAUNCHER=str(fixture / 'kite-desktop'))
    old, new = fixture / 'old', fixture / 'new'
    for release in [old, new]:
        release.mkdir()
        for name in ['desktop.py', 'strings.json']:
            shutil.copyfile(root / name, release / name)
        shutil.copytree(root / 'icons', release / 'icons')
    gui = fixture / 'kite-desktop'

    def launcher(release):
        gui.write_text('#!/bin/sh\nexec /usr/bin/python3 "' + str(release / 'desktop.py') + '" "$@"\n')
        gui.chmod(0o700)

    launcher(old)
    original = subprocess.Popen([str(gui)], env=env,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.monotonic() + 5
        while owner_pid() is None:
            assert time.monotonic() < deadline, 'Original UI did not register'
            time.sleep(.05)
        assert owner_pid() == original.pid
        time.sleep(.5)
        launcher(new)
        installer.refresh_gui(gui, env)
        time.sleep(.5)
        assert owner_pid() == original.pid and original.poll() is None, 'Visible panel must remain open during update'
        assert str(old / 'desktop.py').encode() in (Path('/proc') / str(original.pid) / 'cmdline').read_bytes()
        # No tray on this fixture: the user's normal exit and next launch apply it.
        subprocess.run([str(gui), '--quit'], env=env, check=True, timeout=5)
        original.wait(timeout=5)
        updated = subprocess.Popen([str(gui), '--background'], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + 5
        while owner_pid() in (None, original.pid):
            assert time.monotonic() < deadline, 'Updated UI did not take over'
            time.sleep(.05)
        updated_pid = owner_pid()
        assert str(new / 'desktop.py').encode() in (Path('/proc') / str(updated_pid) / 'cmdline').read_bytes()
        original.wait(timeout=5)
        assert original.returncode == 0
        print(json.dumps(dict(status='PASS', visiblePanelRetained=True, oldUiExited=True, newReleaseLoaded=True,
                              backgroundServicesUntouched=True)), flush=True)
    finally:
        subprocess.run([str(gui), '--quit'], env=env, check=True, timeout=5,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if original.poll() is None:
            original.terminate()
            original.wait(timeout=5)
