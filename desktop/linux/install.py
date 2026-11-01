#!/usr/bin/env python3
"""Install a user-owned Kite node/panel, adopting explicitly named existing services."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parent
APP_ID = 'io.github.wenshiqi0.Kite.Host'


def atomic(path, data, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as file:
        temporary = Path(file.name)
        try:
            file.write(data)
            file.flush()
            os.fsync(file.fileno())
            os.chmod(temporary, mode)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def running_node_matches(expected):
    """The installed symlink may already be new after an interrupted upgrade."""
    result = subprocess.run(['systemctl', '--user', 'show', 'kite-node.service',
                             '--property=MainPID', '--value'], capture_output=True, text=True,
                            check=True, timeout=5)
    try:
        pid = int(result.stdout.strip())
        return pid > 0 and digest(Path('/proc') / str(pid) / 'exe') == expected
    except (OSError, ValueError):
        return False


def wait_for_node(command, version, expected):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        try:
            result = subprocess.run([*command, 'api-version'], capture_output=True, text=True,
                                    timeout=min(6, deadline - time.monotonic()), check=True)
            value = json.loads(result.stdout)
            if value['version'] == version and value['api'] == 1 and digest(Path('/proc') / str(int(value['pid'])) / 'exe') == expected:
                return
        except (OSError, ValueError, KeyError, subprocess.SubprocessError):
            pass
        time.sleep(.1)
    raise RuntimeError('Matching Kite node listener did not become ready')


def refresh_gui(gui, environment):
    """Tell a capable running UI to apply its launcher when it becomes idle."""
    env = dict(os.environ, **environment)
    if not env.get('DBUS_SESSION_BUS_ADDRESS'):
        return
    script = """from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
owner = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
    'org.freedesktop.DBus', 'NameHasOwner', GLib.Variant('(s)', ('io.github.wenshiqi0.Kite.Host',)),
    GLib.VariantType.new('(b)'), Gio.DBusCallFlags.NONE, 1500, None).unpack()[0]
if owner:
    actions = bus.call_sync('io.github.wenshiqi0.Kite.Host', '/io/github/wenshiqi0/Kite/Host',
        'org.gtk.Actions', 'List', None, GLib.VariantType.new('(as)'),
        Gio.DBusCallFlags.NONE, 1500, None).unpack()[0]
    if 'upgrade' in actions:
        bus.call_sync('io.github.wenshiqi0.Kite.Host', '/io/github/wenshiqi0/Kite/Host',
            'org.gtk.Actions', 'Activate', GLib.Variant('(sava{sv})', ('upgrade', [], {})),
            None, Gio.DBusCallFlags.NONE, 1500, None)
"""
    # An unavailable graphical bus must not turn a healthy host upgrade into a failure.
    subprocess.run(['/usr/bin/python3', '-c', script], env=env, capture_output=True, timeout=5)


def configuration(home):
    path = home / '.config/kite/node.json'
    if path.exists():
        return json.loads(path.read_text())
    herdr = shutil.which('herdr') or str(home / '.local/bin/herdr')
    if not Path(herdr).is_file():
        raise RuntimeError('Install herdr first')
    env = {'PATH': str(home / '.local/bin') + ':' + str(home / '.cargo/bin') + ':/usr/local/bin:/usr/bin:/bin',
           'XDG_RUNTIME_DIR': os.environ.get('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))}
    # Adopt the actual capture session without changing the user's systemd environment.
    for proc in Path('/proc').iterdir():
        try:
            if not proc.name.isdigit() or (proc / 'comm').read_text().strip() != 'sunshine':
                continue
            for item in (proc / 'environ').read_bytes().split(b'\0'):
                key, _, value = item.partition(b'=')
                if key.decode() in ['DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS']:
                    env[key.decode()] = value.decode()
            break
        except OSError:
            continue
    return dict(node_id=str(uuid.uuid4()), machine=socket.gethostname(),
                socket=str(home / '.local/state/kite/node/host.sock'),
                backend=str(home / '.local/lib/kite/backend-v5.py'), herdr=herdr,
                python='/usr/bin/python3', systemctl=shutil.which('systemctl') or '/usr/bin/systemctl',
                environment=env, services=[dict(name='Herdr', unit='herdr.service', external=True),
                                           dict(name='Sunshine', unit='app-dev.lizardbyte.app.Sunshine.service', external=True)])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'kite-node')
    parser.add_argument('--enable', action='store_true')
    parser.add_argument('--skill-dir', action='append', type=Path, default=[], help='Install the shared Kite SKILL.md into a harness discovery root')
    parser.add_argument('--migrate-node', action='store_true', help='Allow the first maintenance switch from a node without live handover')
    args = parser.parse_args()
    home = Path.home()
    binary = args.binary.resolve()
    expected = digest(binary)
    manifest_path = ROOT / 'host-manifest.json'
    if manifest_path.exists() and expected != json.loads(manifest_path.read_text())['binarySha256']:
        raise RuntimeError('Host binary checksum mismatch')
    version = json.loads(subprocess.check_output([str(binary), 'version'], text=True, timeout=5))['version']
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', version):
        raise ValueError('Invalid release version')
    config = configuration(home)
    if not all(re.fullmatch(r'[A-Za-z0-9@_.][A-Za-z0-9@_.-]*\.service', s['unit']) for s in config['services']):
        raise ValueError('Invalid service unit')
    unit = home / '.config/systemd/user/kite-node.service'
    socket_unit = unit.with_suffix('.socket')
    link = home / '.local/bin/kite-node'
    command = [str(link), '--socket', config['socket']]
    migration = False
    if args.enable:
        active = subprocess.run(['systemctl', '--user', 'show', 'kite-node.service',
                                 '--property=ActiveState', '--value'], capture_output=True, text=True, timeout=5)
        if active.stdout.strip() == 'active':
            try:
                result = subprocess.run([*command, 'api-version'], capture_output=True,
                                        text=True, check=True, timeout=6)
                migration = not json.loads(result.stdout).get('liveUpgrade', False)
            except (OSError, ValueError, subprocess.SubprocessError):
                migration = True
        if migration and not args.migrate_node:
            raise RuntimeError('Running node has no live handover. First migration requires --migrate-node during a maintenance window; existing connections will reconnect.')
    # Read every asset and validate configuration before changing installed launchers.
    files = {name: (ROOT / name).read_bytes() for name in ['desktop.py', 'strings.json']}
    files['skills/kite/SKILL.md']=(ROOT/'skills/kite/SKILL.md').read_bytes()
    skill_digest=hashlib.sha256(files['skills/kite/SKILL.md']).hexdigest()
    skill_targets=[directory.expanduser().resolve()/'kite' for directory in args.skill_dir]
    for target in skill_targets:
        existing=target/'SKILL.md'
        marker=target/'.kite-source.sha256'
        if existing.exists() and existing.read_bytes()!=files['skills/kite/SKILL.md']:
            if not marker.is_file() or digest(existing)!=marker.read_text().strip():
                raise RuntimeError('Existing Kite skill has local changes; preserve it before installing: '+str(existing))
    alias=home/'.local/bin/kite'
    if alias.exists() or alias.is_symlink():
        if not alias.is_symlink() or alias.resolve()!=link.resolve():
            raise RuntimeError('The kite command already belongs to another installation: '+str(alias))
    for state in ['ready', 'degraded', 'offline', 'busy']:
        name = 'icons/kite-' + state + '.png'
        files[name] = (ROOT / name).read_bytes()
    release = home / '.local/lib/kite-desktop' / version
    release.mkdir(parents=True, exist_ok=True, mode=0o700)
    atomic(release / 'kite-node', binary.read_bytes(), 0o700)
    for name, data in files.items():
        atomic(release / name, data, 0o700 if name.endswith('.py') else 0o644)
    link = home / '.local/bin/kite-node'
    link.parent.mkdir(parents=True, exist_ok=True)
    pending = link.with_name('kite-node.new')
    pending.unlink(missing_ok=True)
    pending.symlink_to(release / 'kite-node')
    pending.replace(link)
    pending=alias.with_name('kite.new')
    pending.unlink(missing_ok=True)
    pending.symlink_to('kite-node')
    pending.replace(alias)
    for target in skill_targets:
        atomic(target/'SKILL.md',files['skills/kite/SKILL.md'],0o644)
        atomic(target/'.kite-source.sha256',(skill_digest+'\n').encode(),0o644)
    gui = home / '.local/bin/kite-desktop'
    atomic(gui, ('#!/bin/sh\nexec /usr/bin/python3 ' + "'" + str(release / 'desktop.py').replace("'", "'\\''") + "'" + ' "$@"\n').encode(), 0o700)
    config_path = home / '.config/kite/node.json'
    if not config_path.exists():
        atomic(config_path, (json.dumps(config, indent=2) + '\n').encode())
    unit = home / '.config/systemd/user/kite-node.service'
    units = ' '.join(s['unit'] for s in config['services'])
    quote = lambda p: '"' + str(p).replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%') + '"'
    contents = '[Unit]\nDescription=Kite desktop host node\nRequires=kite-node.socket\nAfter=kite-node.socket network.target ' + units + '\nWants=' + units + '\n\n[Service]\nEnvironmentFile=-%h/.config/kite/harness.env\nType=notify\nNotifyAccess=all\nSockets=kite-node.socket\nStandardInput=socket\nStandardOutput=journal\nStandardError=journal\nExecStart=' + quote(link) + ' serve --config ' + quote(config_path) + ' --activated\nRestart=on-failure\nRestartSec=2\nUMask=0077\nKillMode=process\nTimeoutStartSec=30\n\n[Install]\nWantedBy=default.target\n'
    socket_contents = '[Unit]\nDescription=Kite private host listener\n\n[Socket]\nListenStream=' + str(config['socket']).replace('\\', '\\\\').replace('%', '%%') + '\nSocketMode=0600\nDirectoryMode=0700\nRemoveOnStop=true\n\n[Install]\nWantedBy=sockets.target\n'
    atomic(socket_unit, socket_contents.encode())
    atomic(unit, contents.encode())
    desktop_quote = lambda path: '"' + str(path).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%') + '"'
    desktop_entry = '[Desktop Entry]\nType=Application\nName=Kite\nComment=Kite Linux host services\nExec=' + desktop_quote(gui) + '\nIcon=' + str(release / 'icons/kite-ready.png') + '\nTerminal=false\nCategories=Network;RemoteAccess;\n'
    atomic(home / '.local/share/applications/io.github.wenshiqi0.Kite.desktop', desktop_entry.encode(), 0o644)
    autostart = home / '.config/autostart/io.github.wenshiqi0.Kite.desktop'
    if not autostart.exists():
        atomic(autostart, desktop_entry.replace('Exec=' + desktop_quote(gui), 'Exec=' + desktop_quote(gui) + ' --background').encode(), 0o644)
    if args.enable and migration:
        subprocess.run(['systemctl', '--user', 'stop', 'kite-node.service'], check=True, timeout=30)
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True, timeout=10)
    if args.enable:
        subprocess.run(['systemctl', '--user', 'enable', '--now', 'kite-node.socket'], check=True, timeout=10)
        subprocess.run(['systemctl', '--user', 'enable', '--now', 'kite-node.service'], check=True, timeout=30)
        if not running_node_matches(expected):
            subprocess.run([*command, 'upgrade', '--binary', str(release / 'kite-node')], check=True, timeout=65)
        wait_for_node(command, version, expected)
        subprocess.run([*command, 'ensure', 'all'], check=True, timeout=35)
        refresh_gui(gui, config.get('environment', {}))
    print(json.dumps(dict(version=version, node=str(link), desktop=str(gui), service=str(unit))))


if __name__ == '__main__':
    main()
