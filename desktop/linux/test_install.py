"""Upgrade recovery with an isolated install tree and a controlled service manager."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('kite_install', Path(__file__).with_name('install.py'))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallTests(unittest.TestCase):
    def test_failed_upgrade_retries_against_running_binary_and_preserves_preferences(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / 'user'
            source = Path(directory) / 'package'
            source.mkdir()
            (source / 'kite-node').write_bytes(b'new binary')
            (source / 'kite-desktop').write_bytes(b'new native desktop')
            (source/'skills/kite').mkdir(parents=True)
            (source/'skills/kite/SKILL.md').write_bytes((Path(__file__).resolve().parents[2]/'skills/kite/SKILL.md').read_bytes())
            for name in ['strings.json', *['icons/kite-' + state + '.png' for state in ['ready', 'degraded', 'offline', 'busy']]]:
                target = source / name
                target.parent.mkdir(exist_ok=True)
                target.write_bytes((installer.ROOT / name).read_bytes())
            link = home / '.local/bin/kite-node'
            link.parent.mkdir(parents=True)
            link.write_bytes(b'old binary')
            config = home / '.config/kite/node.json'
            config.parent.mkdir(parents=True)
            config_bytes = b'{"socket":"/private/custom.sock", "services": [], "environment": {}}\n'
            config.write_bytes(config_bytes)
            autostart = home / '.config/autostart/io.github.wenshiqi0.Kite.desktop'
            autostart.parent.mkdir(parents=True)
            autostart.write_text('[Desktop Entry]\nHidden=true\n')
            calls = []

            def run(command, **kwargs):
                calls.append(command)
                return subprocess.CompletedProcess(command, 0, '', '')

            with patch.object(installer, 'ROOT', source), patch.object(installer.shutil,'which', return_value='/owned/bun'), patch.object(Path, 'home', return_value=home), \
                    patch('sys.argv', ['install.py', '--enable']), \
                    patch.object(installer.subprocess, 'check_output', return_value='{"version":"test-alpha.1"}'), \
                    patch.object(installer, 'running_node_matches', return_value=False) as running, \
                    patch.object(installer, 'wait_for_node') as ready, \
                    patch.object(installer, 'refresh_gui') as refresh, contextlib.redirect_stdout(io.StringIO()):
                def interrupted(command, **kwargs):
                    if 'daemon-reload' in command:
                        raise RuntimeError('interrupted daemon-reload')
                    return subprocess.CompletedProcess(command, 0, '', '')

                with patch.object(installer.subprocess, 'run', side_effect=interrupted):
                    with self.assertRaisesRegex(RuntimeError, 'interrupted'):
                        installer.main()
                self.assertEqual(link.read_bytes(), b'new binary')
                with patch.object(installer.subprocess, 'run', side_effect=run):
                    installer.main()
                self.assertIn([str(link), '--socket', '/private/custom.sock', 'upgrade', '--binary', str(link.resolve())], calls)
                self.assertFalse(any('restart' in call for call in calls))
                self.assertIn([str(link), '--socket', '/private/custom.sock', 'ensure', 'all'], calls)
                running.assert_called_once_with(installer.digest(link))
                ready.assert_called_once()
                refresh.assert_called_once()
            self.assertEqual(json.loads(config.read_text())['bun'],'/owned/bun')
            self.assertEqual(json.loads(config.read_text())['socket'],'/private/custom.sock')
            self.assertIn('Hidden=true', autostart.read_text())

    def test_running_binary_is_read_from_proc_instead_of_installed_symlink(self):
        result = subprocess.CompletedProcess([], 0, str(os.getpid()), '')
        with patch.object(installer.subprocess, 'run', return_value=result):
            self.assertTrue(installer.running_node_matches(installer.digest(Path('/proc/self/exe'))))
            self.assertFalse(installer.running_node_matches('0' * 64))

    def test_gui_update_never_forces_a_restart_or_quits_the_existing_panel(self):
        with patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, '', '')) as run, \
                patch.object(installer.subprocess, 'Popen') as launch:
            installer.refresh_gui(Path('/owned/bin/kite-desktop'), {'DBUS_SESSION_BUS_ADDRESS': 'unix:path=/owned/bus'})
        self.assertEqual(len(run.call_args_list), 1)
        self.assertEqual(run.call_args.args[0][0], 'gdbus')
        launch.assert_not_called()

    def test_live_migration_is_explicit_and_refuses_before_changing_files(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            binary = home / 'candidate'
            binary.write_bytes(b'new binary')
            link = home / '.local/bin/kite-node'
            link.parent.mkdir(parents=True)
            link.write_bytes(b'old binary')
            config = home / '.config/kite/node.json'
            config.parent.mkdir(parents=True)
            config.write_text('{"socket":"/owned/socket","services":[]}')

            def run(command, **kwargs):
                output = 'active' if command[0] == 'systemctl' else '{"api":1}'
                return subprocess.CompletedProcess(command, 0, output, '')

            with patch.object(Path, 'home', return_value=home), \
                    patch('sys.argv', ['install.py', '--binary', str(binary), '--enable']), \
                    patch.object(installer.subprocess, 'run', side_effect=run), \
                    patch.object(installer.subprocess, 'check_output', return_value='{"version":"test-alpha.1"}'):
                with self.assertRaisesRegex(RuntimeError, '--migrate-node'):
                    installer.main()
            self.assertEqual(link.read_bytes(), b'old binary')
            self.assertFalse((home / '.config/systemd').exists())


if __name__ == '__main__':
    unittest.main()
