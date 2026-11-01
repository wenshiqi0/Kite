#!/usr/bin/env python3
"""Native regressions for overlapping queries, retained errors and hidden panels."""
import importlib.util
import argparse
import json
import os
from pathlib import Path
import sys
import tempfile
import time
from unittest.mock import patch

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--evidence', type=Path)
args = parser.parse_args()
if args.evidence:
    args.evidence.mkdir(parents=True, exist_ok=True)
temporary = tempfile.TemporaryDirectory(prefix='kite-operations-probe-')
fixture = Path(temporary.name)
binary = fixture / 'node'
binary.write_text('''#!/usr/bin/python3
import json,sys,time
from pathlib import Path
root=Path(__file__).parent
action=sys.argv[1]
with (root/'calls').open('a') as out:out.write(action+'\\n')
if action=='status':
 delay=root/'delay'
 if delay.exists():time.sleep(float(delay.read_text()))
 print(json.dumps(dict(health='ready',machine='Owned QA host',version='operations-probe',services=[dict(name='Herdr',unit='herdr.service',active=True)])))
elif action=='ensure':
 time.sleep(.8)
 if (root/'fail').exists():sys.stderr.write('Owned service start failed');sys.exit(2)
 print('{}')
elif action=='sessions':print('{"sessions":[]}')
else:sys.exit(2)
''')
binary.chmod(0o700)
os.environ['KITE_NODE_BINARY'] = str(binary)
spec = importlib.util.spec_from_file_location('kite_desktop', root / 'desktop.py')
ui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui)
app = ui.Application()
failure = None
deadline = time.monotonic() + 25


def close_dialogs():
    for window in app.get_windows():
        if isinstance(window, ui.Gtk.MessageDialog):
            window.response(ui.Gtk.ResponseType.CLOSE)


def capture_widgets(name):
    # Render actual GTK widgets without occlusion by other X11/portal windows.
    import cairo
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, app.window.get_allocated_width(),
                                 app.window.get_allocated_height())
    app.window.draw(cairo.Context(surface))
    surface.write_to_png(str(args.evidence / name))


def scenario():
    yield lambda: app.window is not None and app.state == 'ready' and not app.pending
    rows = tuple(app.window.service_rows.values())
    (fixture / 'delay').write_text('.8')
    app.call('status')
    app.call('sessions')
    yield lambda: not app.action_pending and 'sessions' in (fixture / 'calls').read_text()
    assert app.status_pending, 'User action must run during a slow status query'
    (fixture / 'delay').write_text('0')
    yield lambda: not app.pending
    assert tuple(app.window.service_rows.values()) == rows, 'Stable services must reuse their GTK rows'

    (fixture / 'fail').touch()
    sessions_before = (fixture / 'calls').read_text().splitlines().count('sessions')
    app.call('ensure', 'all')
    app.call('sessions')
    app.call('sessions')
    assert list(app.action_queue) == [('sessions',)], 'Repeated queued actions must coalesce'
    app.call('status')
    yield lambda: not app.status_pending
    assert app.action_pending, 'Health refresh must continue during service startup'
    yield lambda: not app.pending
    assert (fixture / 'calls').read_text().splitlines().count('sessions') == sessions_before + 1
    assert app.state == 'ready' and 'Owned service start failed' in app.action_error
    assert app.window.error_box.get_visible()
    if args.evidence:
        app.window.queue_draw()
        drawn_at = time.monotonic() + .25
        yield lambda: time.monotonic() >= drawn_at
        capture_widgets('operation-error.png')
    app.call('status')
    yield lambda: not app.pending
    assert 'Owned service start failed' in app.action_error, 'Healthy polling must retain action failures'

    (fixture / 'fail').unlink()
    app.call('ensure', 'all')
    yield lambda: not app.pending
    assert app.action_error is None, 'Successful retry must clear its own failure'
    if args.evidence:
        app.window.queue_draw()
        drawn_at = time.monotonic() + .25
        yield lambda: time.monotonic() >= drawn_at
        capture_widgets('recovered.png')

    app.window.hide()
    with patch.object(app.window, 'render', wraps=app.window.render) as render:
        app.render()
        render.assert_not_called()
    with patch.object(ui.GLib, 'timeout_add_seconds', return_value=0) as timer:
        app.schedule_poll()
        assert timer.call_args.args[0] == 15
        app.window.show()
        app.schedule_poll()
        assert timer.call_args.args[0] == 3
    app.action_error = 'Owned retained error'
    app.dismiss_error()
    assert app.action_error is None and not app.window.error_box.get_visible()
    launcher = fixture / 'launcher'
    launcher.write_bytes(b'updated launcher')
    app.launcher, app.launcher_bytes = launcher, b'original launcher'
    with patch.object(app, 'embedded', return_value=True), patch.object(ui.os, 'execv') as execute:
        app.check_ui_upgrade()
        assert app.update_pending
        execute.assert_not_called()
        app.window.hide()
        app.action_pending = True
        app.check_ui_upgrade()
        execute.assert_not_called()
        app.action_pending = False
        app.check_ui_upgrade()
        execute.assert_called_once_with(str(launcher), [str(launcher), '--background'])
    app.launcher_bytes = None
    print(json.dumps(dict(status='PASS', overlappingStatusAndActions=True,
                          queuedActions=True, persistentErrors=True, successfulRetry=True,
                          stableWidgets=True, hiddenWindowSkipsRendering=True,
                          backgroundPollSeconds=15, guiUpgradeWaitsForIdle=True)), flush=True)
    app.quit()


steps = scenario()
waiting = lambda: True


def step():
    global waiting, failure
    try:
        assert time.monotonic() < deadline, 'Operations probe deadline exceeded'
        close_dialogs()
        if waiting():
            waiting = next(steps)
        return True
    except StopIteration:
        return False
    except Exception as error:
        failure = repr(error)
        print(failure, file=sys.stderr, flush=True)
        app.quit()
        return False


ui.GLib.timeout_add(50, step)
app.run([str(root / 'desktop.py')])
temporary.cleanup()
sys.exit(1 if failure else 0)
