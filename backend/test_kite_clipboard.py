import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import kite_clipboard as clipboard


class ClipboardTests(unittest.TestCase):
    def fixture(self, directory):
        program = Path(directory)/"clip"
        program.write_text("#!"+sys.executable+"\nimport os,sys\nfrom pathlib import Path\np=Path(os.environ['KITE_CLIP_TEST_FILE'])\nif '-out' in sys.argv:sys.stdout.buffer.write(p.read_bytes())\nelse:p.write_bytes(sys.stdin.buffer.read())\n")
        program.chmod(0o700)
        return program, dict(os.environ, DISPLAY=":controlled", KITE_CLIP_TEST_FILE=str(Path(directory)/"value"))

    def test_unicode_text_round_trip_and_empty_clipboard_use_actual_pipes(self):
        with tempfile.TemporaryDirectory() as folder:
            program, env = self.fixture(folder)
            with patch.object(clipboard,"executable",return_value=str(program)):
                text="来自手机\nline two\n"
                clipboard.transfer("write",text.encode(),env,"linux")
                self.assertEqual(clipboard.transfer("read",b"",env,"linux"),text)
                clipboard.transfer("write",b"",env,"linux")
                self.assertEqual(clipboard.transfer("read",b"",env,"linux"),"")

    def test_oversized_writes_never_start_a_process_and_reads_are_bounded(self):
        with patch.object(clipboard.subprocess,"Popen") as process:
            with self.assertRaises(ValueError):clipboard.transfer("write",b"x"*(clipboard.LIMIT+1),{},"linux")
            process.assert_not_called()
        with tempfile.TemporaryDirectory() as folder:
            program, env = self.fixture(folder)
            Path(env["KITE_CLIP_TEST_FILE"]).write_bytes(b"x"*(clipboard.LIMIT+1024))
            with patch.object(clipboard,"executable",return_value=str(program)), self.assertRaisesRegex(ValueError,"exceeds limit"):
                clipboard.transfer("read",b"",env,"linux")

    def test_ambiguous_desktop_identity_is_rejected(self):
        from types import SimpleNamespace
        with patch.object(clipboard.sys,"platform","linux"), patch.object(clipboard.subprocess,"run",return_value=SimpleNamespace(stdout='users:(("sunshine",pid=1,fd=3),("sunshine",pid=2,fd=4))')):
            with self.assertRaisesRegex(RuntimeError,"process unavailable"):clipboard.environment(47989)

    def test_non_text_selection_is_empty_but_display_failures_are_not_hidden(self):
        with tempfile.TemporaryDirectory() as folder:
            program=Path(folder)/"clip"
            for diagnostic, empty in [("Error: target UTF8_STRING not available",True),("Error: Can't open display: :controlled",False)]:
                program.write_text("#!"+sys.executable+"\nimport sys\nprint("+repr(diagnostic)+",file=sys.stderr)\nsys.exit(1)\n")
                program.chmod(0o700)
                with patch.object(clipboard,"executable",return_value=str(program)):
                    if empty:self.assertEqual(clipboard.transfer("read",b"",dict(os.environ,DISPLAY=":controlled"),"linux"),"")
                    else:
                        with self.assertRaisesRegex(RuntimeError,"operation failed"):clipboard.transfer("read",b"",dict(os.environ,DISPLAY=":controlled"),"linux")


if __name__=="__main__":unittest.main()
