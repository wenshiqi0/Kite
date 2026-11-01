import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('kite_desktop_adapter',Path(__file__).with_name('kite_desktop.py'))
adapter=importlib.util.module_from_spec(spec);spec.loader.exec_module(adapter)


class DesktopEnvironmentTests(unittest.TestCase):
    def test_apps_do_not_inherit_node_supervision_or_another_agents_identity(self):
        env={'DISPLAY':':owned','NOTIFY_SOCKET':'/owned/manager','LISTEN_FDS':'99',
             'HERDR_PANE_ID':'unrelated','CODEX_THREAD_ID':'unrelated','CHROME_USER_DATA_DIR':'unrelated'}
        with patch.dict(os.environ,env,clear=True),patch.object(adapter.Path,'iterdir',return_value=[]):
            result=adapter.session_environment()
        self.assertEqual(result['DISPLAY'],':owned')
        for key in env.keys()-{'DISPLAY'}:self.assertNotIn(key,result)

    def test_unsupported_wayland_and_missing_targets_are_not_silently_selected(self):
        for env in [{},{'DISPLAY':':owned','XDG_SESSION_TYPE':'wayland'}]:
            with patch.dict(os.environ,env,clear=True),patch.object(adapter.Path,'iterdir',return_value=[]):
                with self.assertRaises(adapter.DesktopError):adapter.session_environment()


if __name__=='__main__':unittest.main()
