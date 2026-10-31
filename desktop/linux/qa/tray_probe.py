#!/usr/bin/env python3
"""Opt-in native GTK acceptance. Isolated fixture; never stops real host services."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

p = argparse.ArgumentParser()
p.add_argument('--require-tray', action='store_true')
p.add_argument('--evidence', type=Path)
args = p.parse_args()
if args.evidence:
    args.evidence.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[1]
temporary = tempfile.TemporaryDirectory(prefix='kite-tray-probe-')
fixture = Path(temporary.name)
state = fixture / 'state'
state.write_text('ready')
binary = fixture / 'node'
binary.write_text('''#!/usr/bin/python3
import json,sys,time
from pathlib import Path
root=Path(__file__).parent
with (root/'calls').open('a') as out:out.write(json.dumps(sys.argv[1:])+'\\n')
mode=(root/'state').read_text()
if sys.argv[1:] == ['ensure','all']:
 (root/'state').write_text('ready');print('{}');sys.exit()
assert sys.argv[1:] == ['status'],sys.argv
if mode == 'offline':sys.stderr.write('Owned fixture disconnected');sys.exit(2)
if mode == 'slow':time.sleep(10)
print(json.dumps(dict(health=mode,machine='Owned QA host',version='native-probe',services=[dict(name='Herdr',active=True),dict(name='Sunshine',active=mode=='ready')])))
''')
binary.chmod(0o700)
os.environ['KITE_NODE_BINARY'] = str(binary)
spec = importlib.util.spec_from_file_location('kite_desktop', root / 'desktop.py')
ui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui)
app = ui.Application()
stage = 0
failure = None
duplicate = None
deadline = time.monotonic() + 35
original_window = None
last_state = None
not_before = 0


def capture(name):
    if args.evidence:
        app.snapshot = str(args.evidence / (name + '.png'))
        app.capture()


def step():
    global stage, failure, duplicate, original_window, last_state, not_before
    try:
        assert time.monotonic() < deadline, ('Deadline', stage, app.state, app.error)
        if app.pending:
            return True
        if app.state != last_state:
            last_state, not_before = app.state, time.monotonic() + .4
            return True
        if time.monotonic() < not_before:
            return True
        if stage == 0 and app.state == 'ready':
            if not app.embedded() and app.window is None:
                return True
            if not app.embedded():
                assert app.window.get_visible(), 'Missing tray must present the panel automatically'
            if app.window is None:
                app.present_window()
                not_before = time.monotonic() + .4
                return True
            app.present_window()
            original_window = app.window
            assert app.window.status.get_text() == ui.TEXT['ready']
            assert 'Sunshine · ' + ui.TEXT['active'] in app.tray.get_tooltip_text()
            if args.require_tray:
                assert app.embedded(), 'The X11 notification area did not embed the icon'
            capture('ready')
            app.tray.emit('popup-menu', 3, ui.Gdk.CURRENT_TIME)
            assert app.menu.get_mapped(), 'Tray context menu did not open'
            app.menu.popdown()
            state.write_text('degraded')
            app.call('status')
            stage = 1
        elif stage == 1 and app.state == 'degraded':
            assert app.window.status.get_text() == ui.TEXT['degraded']
            assert 'Sunshine · ' + ui.TEXT['inactive'] in app.tray.get_tooltip_text()
            assert not app.window.status.get_style_context().has_class('success')
            capture('degraded')
            ensure = next(item for item in app.menu.get_children() if isinstance(item, ui.Gtk.MenuItem) and item.get_label() == ui.TEXT['ensure'])
            assert ensure.get_sensitive()
            ensure.activate()
            stage = 2
        elif stage == 2 and app.state == 'ready':
            state.write_text('offline')
            app.call('status')
            stage = 3
        elif stage == 3 and app.state == 'offline':
            assert app.value is None
            assert not app.window.ensure.get_sensitive()
            assert 'Sunshine · ' + ui.TEXT['active'] not in app.tray.get_tooltip_text()
            assert not app.window.services.get_children()
            capture('offline')
            state.write_text('slow')
            app.call('status')
            stage = 4
        elif stage == 4 and app.state == 'offline':
            assert 'timed out' in app.error
            state.write_text('ready')
            app.call('status')
            stage = 5
        elif stage == 5 and app.state == 'ready':
            app.present_window()
            assert app.window is original_window
            duplicate = subprocess.Popen(['/usr/bin/python3', str(root / 'desktop.py'), '--background'], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stage = 6
        elif stage == 6 and duplicate.poll() is not None:
            out, err = duplicate.communicate()
            assert duplicate.returncode == 0, (out, err)
            assert len(app.get_windows()) == 1
            if app.embedded():
                app.close_panel()
                assert not app.window.get_visible()
                app.tray.emit('activate')
                assert app.window.get_visible() and app.window is original_window
            else:
                assert app.window.get_visible(), 'A missing tray must retain an accessible window'
            command = ['/usr/bin/python3', str(root / 'desktop.py')]
            if args.evidence:
                command += ['--snapshot', str(args.evidence / 'secondary-launch.png')]
            duplicate = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stage = 7
        elif stage == 7 and duplicate.poll() is not None:
            out, err = duplicate.communicate()
            assert duplicate.returncode == 0, (out, err)
            assert app.window is original_window and len(app.get_windows()) == 1
            assert app.window.get_visible()
            calls = [json.loads(line) for line in (fixture / 'calls').read_text().splitlines()]
            assert ['ensure', 'all'] in calls
            assert all(call in (['status'], ['ensure', 'all']) for call in calls)
            result = dict(status='PASS', embedded=app.embedded(), ready=True, degraded=True,
                          offlineClearsStaleState=True, timeout=True, recovered=True,
                          singleInstance=True, sameWindowReopened=True, backgroundServicesUntouched=True)
            if args.evidence:
                (args.evidence / 'native-validation.json').write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result), flush=True)
            app.quit()
            return False
        return True
    except Exception as e:
        failure = repr(e)
        print(failure, file=sys.stderr, flush=True)
        app.quit()
        return False


ui.GLib.timeout_add(100, step)
app.run([str(root / 'desktop.py'), '--background'])
if duplicate and duplicate.poll() is None:
    duplicate.terminate()
    duplicate.wait(timeout=3)
temporary.cleanup()
sys.exit(1 if failure else 0)
