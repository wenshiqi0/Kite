#!/usr/bin/env python3
"""Live handover on owned real user-systemd units; no model/production service."""
import argparse
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import shutil
import socket
import struct
import subprocess
import tempfile
import threading
import time
import uuid
from unittest.mock import patch

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--evidence', type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
source_root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('kite_install', source_root / 'install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
real_run = subprocess.run


def systemctl(*args):
    return subprocess.run(['systemctl', '--user', *args], capture_output=True, text=True,
                          check=True, timeout=15).stdout.strip()


class Bridge:
    def __init__(self, address, herdr):
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.settimeout(5)
        self.socket.connect(str(address))
        self.socket.sendall((json.dumps(dict(method='herdr-api', args=['--herdr', str(herdr)], params={})) + '\n').encode())
        ack = b''
        while not ack.endswith(b'\n'):
            ack += self.socket.recv(1)
        assert json.loads(ack)['ok']
        self.buffer = b''

    def send(self, data, kind=0):
        self.socket.sendall(struct.pack('!BI', kind, len(data)) + data)

    def exact(self, count):
        data = b''
        while len(data) < count:
            chunk = self.socket.recv(count - len(data))
            assert chunk, 'Existing bridge disconnected during upgrade'
            data += chunk
        return data

    def frame(self):
        kind, count = struct.unpack('!BI', self.exact(5))
        assert count <= 65536
        return kind, self.exact(count)

    def line(self):
        while b'\n' not in self.buffer:
            kind, data = self.frame()
            assert kind == 1, (kind, data)
            self.buffer += data
        line, self.buffer = self.buffer.split(b'\n', 1)
        return line

    def close(self):
        self.send(b'', 4)
        kind, data = self.frame()
        assert kind == 3 and data == bytes(4), (kind, data)
        self.socket.close()


systemctl('show-environment')  # Fail clearly if this required integration environment is absent.
unit_name = 'kite-qa-upgrade-' + uuid.uuid4().hex
unit_root = Path(os.environ['XDG_RUNTIME_DIR']) / 'systemd/user'
unit_root.mkdir(parents=True, exist_ok=True)
service_path, socket_path = [unit_root / (unit_name + suffix) for suffix in ['.service', '.socket']]
with tempfile.TemporaryDirectory(prefix='kite-live-upgrade-') as directory:
    root = Path(directory)
    stable_binary=root/'test-node'
    shutil.copyfile(binary,stable_binary);stable_binary.chmod(0o700);binary=stable_binary
    node_dir = root / 'node'
    node_dir.mkdir(mode=0o700)
    address = node_dir / 'host.sock'
    herdr = root / 'herdr'
    (root / 'owner.py').write_text('import time\nfrom pathlib import Path\np=Path(__file__).with_name("heartbeat")\nn=0\nwhile True:\n n+=1;q=p.with_suffix(".pending");q.write_text(str(n));q.replace(p);time.sleep(.02)\n')
    herdr.write_text('''#!/usr/bin/python3
import os,subprocess,sys,time
from pathlib import Path
root=Path(__file__).parent
assert sys.argv[1:]==['remote-api-bridge']
if not (root/'owner.pid').exists():
 child=subprocess.Popen([sys.executable,str(root/'owner.py')],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
 (root/'owner.pid').write_text(str(child.pid))
for line in sys.stdin.buffer:
 if line.startswith(b'MUTATE'):
  with (root/'mutations').open('ab') as out:out.write(line)
  while not (root/'release').exists():time.sleep(.01)
  sys.stdout.buffer.write(b'APPLIED\\n')
 else:sys.stdout.buffer.write(b'PONG '+line.removeprefix(b'PING '))
 sys.stdout.buffer.flush()
''')
    herdr.chmod(0o700)
    manager = root / 'manager'
    manager.mkdir()
    (manager / 'systemctl').symlink_to('/usr/bin/systemctl')
    notify = manager / 'systemd-notify'
    notify.write_text('''#!/usr/bin/python3
import subprocess,sys
from pathlib import Path
flag=Path(__file__).parent.parent/'fail-notify'
fail=flag.exists()
if fail:flag.unlink()
result=subprocess.run(['/usr/bin/systemd-notify',*sys.argv[1:]])
sys.exit(1 if fail else result.returncode)
''')
    notify.chmod(0o700)
    facts = root / 'facts.json'
    fact = dict(session_id='owned-session', title='Owned session', source='owned-source',
                status='working', revision=1, thread='owned-thread', controls='', controls_known=True, lifecycle=False)
    facts.write_text(json.dumps(dict(facts=[fact])))
    interpreter = root / 'interpreter'
    interpreter.write_text('''#!/usr/bin/python3
import json,os,sys,time
from pathlib import Path
root=Path(__file__).parent
assert sys.argv[2]=='notification-facts'
facts=json.loads((root/'facts.json').read_text())
def log(phase):
 with (root/'scans').open('a') as out:out.write(json.dumps(dict(pid=os.getppid(),phase=phase))+'\\n')
log('start')
if (root/'hold-scan').exists():
 (root/'scan-inflight').touch();time.sleep(2)
log('end')
print(json.dumps(facts))
''')
    interpreter.chmod(0o700)
    config = root / 'node.json'
    config.write_text(json.dumps(dict(node_id='owned-upgrade-probe', machine='owned-fixture',
                                     socket=str(address), backend=str(root / 'adapters/host.js'),
                                     herdr=str(herdr), bun=str(interpreter), systemctl=str(manager / 'systemctl'),
                                     services=[], environment={})))
    # The controlled interpreter supplies only owned notification facts; no real
    # catalogue, model, push credentials or provider is queried.
    home, package = root / 'user', root / 'package'
    package.mkdir()
    shutil.copyfile(binary, package / 'kite-node')
    (package / 'kite-node').chmod(0o700)
    (package / 'kite-desktop').write_bytes(b'controlled native GUI')
    for name in ['strings.json']:
        shutil.copyfile(source_root / name, package / name)
    shutil.copytree(source_root / 'icons', package / 'icons')
    shutil.copytree(source_root.parent.parent / 'skills', package / 'skills')
    installed_config = home / '.config/kite/node.json'
    installed_config.parent.mkdir(parents=True)
    shutil.copyfile(config, installed_config)
    manager_calls = []
    upgrading = threading.Event()

    def install_run(command, **kwargs):
        if 'upgrade' in command:
            upgrading.set()
        if command[0] == 'systemctl':
            if 'daemon-reload' in command:
                for kind in ['service', 'socket']:
                    text = (home / ('.config/systemd/user/kite-node.' + kind)).read_text()
                    (unit_root / (unit_name + '.' + kind)).write_text(text.replace('kite-node.socket', unit_name + '.socket'))
            command = [part.replace('kite-node.service', unit_name + '.service').replace('kite-node.socket', unit_name + '.socket') for part in command]
            # The real unit files are tested; this owned fixture never enables a
            # persistent login/boot dependency or targets a production unit name.
            if 'enable' in command:
                command = [part for part in command if part != '--now']
                command[command.index('enable')] = 'start'
            manager_calls.append(command)
        return real_run(command, **kwargs)

    def install():
        with patch.object(installer, 'ROOT', package), patch.object(Path, 'home', return_value=home), \
                patch('sys.argv', ['install.py', '--enable','--skill-dir',str(home/'.agents/skills')]), patch.object(subprocess, 'run', side_effect=install_run), \
                patch.object(installer, 'refresh_gui'), contextlib.redirect_stdout(io.StringIO()):
            installer.main()
    stop = threading.Event()
    errors, samples = [], []
    pump = None
    streams = []
    owner_pid = None
    old_pid = None
    result = None
    try:
        install()

        def status():
            value = subprocess.check_output([str(binary), '--socket', str(address), 'status'], text=True, timeout=6)
            return json.loads(value)

        old_pid = status()['pid']
        installed_skill=home/'.agents/skills/kite/SKILL.md'
        skill_body=installed_skill.read_bytes();installed_skill.write_bytes(skill_body+b'\nOwned custom skill edit\n')
        try:
            install()
            raise AssertionError('An edited user skill must be preserved')
        except RuntimeError as error:
            assert 'local changes' in str(error)
        assert installed_skill.read_bytes().endswith(b'Owned custom skill edit\n') and status()['pid']==old_pid
        installed_skill.write_bytes(skill_body)
        deadline = time.monotonic() + 5
        while not status()['notificationLastScanMs']:
            assert time.monotonic() < deadline
            time.sleep(.02)

        def register(token):
            payload = dict(device_id='owned-device', host_id='owned-host', token=token, project_id='owned-project')
            response = subprocess.run([str(binary), '--socket', str(address), 'push-register'],
                                      input=json.dumps(payload) + '\n', capture_output=True, text=True, check=True, timeout=15)
            assert json.loads(response.stdout)['registered']

        register('owned-token-before')
        live = Bridge(address, herdr)
        streams.append(live)

        def run():
            try:
                sequence = 0
                while not stop.is_set():
                    sent = time.monotonic()
                    live.send(('PING ' + str(sequence) + '\n').encode())
                    assert live.line() == ('PONG ' + str(sequence)).encode()
                    samples.append(time.monotonic() - sent)
                    sequence += 1
                    time.sleep(.01)
            except Exception as error:
                errors.append(repr(error))

        pump = threading.Thread(target=run)
        pump.start()
        deadline = time.monotonic() + 5
        while len(samples) < 20:
            assert time.monotonic() < deadline and not errors, errors
            time.sleep(.02)
        owner_pid = int((root / 'owner.pid').read_text())
        heartbeat_before = int((root / 'heartbeat').read_text())
        once = Bridge(address, herdr)
        streams.append(once)
        once.send(b'MUTATE once\n')
        deadline = time.monotonic() + 5
        while not (root / 'mutations').exists():
            assert time.monotonic() < deadline
            time.sleep(.01)

        for name, metadata in [('incompatible', dict(api=999)), ('broken', dict(api=1, backendProtocol=5, liveUpgrade=1, generationLaunch=1))]:
            bad = root / name
            bad.write_text('#!/usr/bin/python3\nimport sys,json\nif sys.argv[1:]==["version"]:print(json.dumps(' + repr(metadata) + '))\nelse:sys.exit(3)\n')
            bad.chmod(0o700)
            failed = subprocess.run([str(binary), '--socket', str(address), 'upgrade', '--binary', str(bad)], capture_output=True, text=True, timeout=15)
            assert failed.returncode != 0 and status()['pid'] == old_pid

        # Simulate a notifier that delivered MAINPID but failed before confirming
        # it to the upgrader. Old ownership must be restored before candidate exit.
        (root / 'fail-notify').touch()
        failed = subprocess.run([str(binary), '--socket', str(address), 'upgrade', '--binary', str(binary)], capture_output=True, text=True, timeout=30)
        assert failed.returncode != 0 and status()['pid'] == old_pid
        assert int(systemctl('show', unit_name + '.service', '--property=MainPID', '--value')) == old_pid

        saved_registry = (node_dir / 'push-devices.json').read_bytes()
        (node_dir / 'push-devices.json').write_text('{invalid-owned-fixture')
        failed = subprocess.run([str(binary), '--socket', str(address), 'upgrade', '--binary', str(binary)], capture_output=True, text=True, timeout=15)
        assert failed.returncode != 0 and status()['pid'] == old_pid
        (node_dir / 'push-devices.json').write_bytes(saved_registry)

        (root / 'hold-scan').touch()
        deadline = time.monotonic() + 5
        while not (root / 'scan-inflight').exists():
            assert time.monotonic() < deadline
            time.sleep(.01)

        registration_errors = []
        def rotate_registration():
            try:
                assert upgrading.wait(5)
                time.sleep(.05)
                register('owned-token-after')
            except Exception as error:
                registration_errors.append(repr(error))
        rotation = threading.Thread(target=rotate_registration)
        rotation.start()

        upgrade_started = time.monotonic()
        # Different executable bytes force an actual installer upgrade. Linux
        # ignores the owned trailing ELF marker; no second model/build is needed.
        with (package / 'kite-node').open('ab') as updated_binary:
            updated_binary.write(b'owned-new-generation')
        install()
        rotation.join(timeout=15)
        assert not rotation.is_alive() and not registration_errors, registration_errors
        upgraded_in = time.monotonic() - upgrade_started
        new_pid = status()['pid']
        assert new_pid != old_pid
        assert not any('restart' in call or 'stop' in call for call in manager_calls)
        assert int(systemctl('show', unit_name + '.service', '--property=MainPID', '--value')) == new_pid
        (root / 'hold-scan').unlink()
        fact.update(status='idle', revision=2)
        facts.write_text(json.dumps(dict(facts=[fact])))
        deadline = time.monotonic() + 7
        while True:
            page = json.loads(subprocess.check_output([str(binary), '--socket', str(address), 'notifications', '--after', 'owned-upgrade-probe:0'], text=True, timeout=6))
            if page['events']:
                assert len(page['events']) == 1
                break
            assert time.monotonic() < deadline, 'Completion notification was lost during handover'
            time.sleep(.05)
        state = json.loads((node_dir / 'push-devices.json').read_text())
        assert state['devices']['owned-device/owned-host']['token'] == 'owned-token-after'
        scans = [json.loads(line) for line in (root / 'scans').read_text().splitlines()]
        old_end = max(i for i, scan in enumerate(scans) if scan['pid'] == old_pid and scan['phase'] == 'end')
        new_start = min(i for i, scan in enumerate(scans) if scan['pid'] == new_pid and scan['phase'] == 'start')
        assert old_end < new_start, 'Old/new notification writers overlapped'
        assert Path('/proc', str(old_pid)).exists(), 'Old generation must retain its live streams'
        manager_pid = Path('/proc', str(old_pid), 'stat').read_text().rsplit(') ', 1)[1].split()[1]
        parent_pid = Path('/proc', str(new_pid), 'stat').read_text().rsplit(') ', 1)[1].split()[1]
        assert parent_pid == manager_pid, 'New generation must be adopted by the service manager before commit'
        crashed_pid = new_pid
        os.kill(crashed_pid, signal.SIGKILL)
        recover_deadline = time.monotonic() + 8
        while True:
            current = int(systemctl('show', unit_name + '.service', '--property=MainPID', '--value'))
            if current not in (0, crashed_pid):
                try:
                    if status()['pid'] == current:
                        new_pid = current
                        break
                except (subprocess.SubprocessError, ValueError):
                    pass
            assert time.monotonic() < recover_deadline, 'Supervisor did not detect new-generation death while old bridges stayed open'
            time.sleep(.05)
        assert Path('/proc', str(old_pid)).exists(), 'Crash recovery must retain old-generation connections'
        after = Bridge(address, herdr)
        after.send(b'PING new\n')
        assert after.line() == b'PONG new'
        after.close()
        (root / 'release').touch()
        assert once.line() == b'APPLIED'
        assert (root / 'mutations').read_bytes() == b'MUTATE once\n'
        assert int((root / 'owner.pid').read_text()) == owner_pid
        assert int((root / 'heartbeat').read_text()) > heartbeat_before
        os.kill(owner_pid, 0)
        time.sleep(.2)
        assert not errors, errors
        stop.set()
        pump.join(timeout=6)
        assert not pump.is_alive()
        for stream in streams:
            stream.close()
        streams.clear()
        deadline = time.monotonic() + 5
        while Path('/proc', str(old_pid)).exists():
            assert time.monotonic() < deadline, 'Old generation did not exit after draining'
            time.sleep(.02)
        assert status()['pid'] == new_pid
        result = dict(status='PASS', realUserSystemd=True, continuousFrames=len(samples),
                      frameErrors=errors, maxRoundTripMs=round(max(samples) * 1000, 2),
                      upgradeSeconds=round(upgraded_in, 3), inFlightMutationExactlyOnce=True,
                      sameOwnedAgentPid=True, newConnectionsUseNewGeneration=True,
                      incompatibleAndBrokenCandidatesRetainOld=True, oldGenerationDrained=True)
        result['notifierFailureRestoresOldMainPid'] = True
        result['realInstallerWithoutRestart'] = True
        result['corruptCandidateStateRetainsOld'] = True
        result['notificationWriterHandoverWithoutDuplicate'] = True
        result['concurrentPushRegistrationRetained'] = True
        result['replacementAdoptedByUserManager'] = True
        result['newGenerationCrashAutomaticallyRecovered'] = True
        result['customizedSkillPreserved'] = True
        print(json.dumps(result), flush=True)
        if args.evidence:
            args.evidence.parent.mkdir(parents=True, exist_ok=True)
            args.evidence.write_text(json.dumps(result, indent=2) + '\n')
    finally:
        stop.set()
        if pump:
            pump.join(timeout=6)
        for stream in streams:
            stream.socket.close()
        if owner_pid is None and (root / 'owner.pid').exists():
            owner_pid = int((root / 'owner.pid').read_text())
        if owner_pid:
            try:
                os.kill(owner_pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        try:
            real_run(['systemctl', '--user', 'stop', unit_name + '.service', unit_name + '.socket'],
                     capture_output=True, timeout=15)
        finally:
            service_path.unlink(missing_ok=True)
            socket_path.unlink(missing_ok=True)
            systemctl('daemon-reload')
            real_run(['systemctl', '--user', 'reset-failed', unit_name + '.service', unit_name + '.socket'],
                     capture_output=True, timeout=5)
