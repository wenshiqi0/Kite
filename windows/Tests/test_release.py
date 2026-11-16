"""Release routing and immutable Windows package checks; no network writes."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import Mock, patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
targets = runpy.run_path(str(ROOT / "scripts/release-targets.py"))
publisher = runpy.run_path(str(ROOT / "windows/scripts/release.py"))
VERSION = "0.2.0-alpha.104"
COMMIT = "a" * 40


def package(directory, *, version=VERSION, commit=COMMIT, dirty=False, tamper=False, extra=False):
    archive = directory / "Kite-windows-x64.zip"
    files = {"Kite.exe": b"owned executable fixture", "Kite.deps.json": json.dumps({
        "libraries": {"Kite/" + VERSION: {}}}).encode()}
    manifest = {"source_commit": commit, "version": version, "dirty": dirty,
        "target": "win-x64", "built_on": "linux", "host_role": False,
        "sha256": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
    with zipfile.ZipFile(archive, "w") as bundle:
        for name, data in files.items():
            bundle.writestr("Kite/" + name, b"changed" if tamper and name == "Kite.exe" else data)
        bundle.writestr("Kite/build-manifest.json", json.dumps(manifest))
        if extra:
            bundle.writestr("Kite/unknown.exe", b"unrecorded")
    checksum = Path(str(archive) + ".sha256")
    checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
    return archive, checksum


class ReleaseTests(unittest.TestCase):
    def test_windows_tag_excludes_android_and_host_publication(self):
        self.assertEqual(targets["select_targets"]("tag", targets["WINDOWS_MARKER"]),
                         {"android": False, "windows": True})
        self.assertEqual(targets["select_targets"]("tag", "ordinary Alpha"),
                         {"android": True, "windows": False})
        self.assertEqual(targets["select_targets"]("branch"), {"android": True, "windows": True})

    def test_unknown_and_duplicate_release_markers_fail_closed(self):
        for notes in ("<!-- Kite-Release-Targets: typo -->", "\n".join([targets["WINDOWS_MARKER"]] * 2)):
            with self.subTest(notes=notes), self.assertRaises(ValueError):
                targets["select_targets"]("tag", notes)

    def test_valid_package_preserves_exact_ci_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            archive, checksum = package(Path(directory))
            manifest, digest = publisher["verify_package"](archive, checksum, VERSION, COMMIT)
            self.assertEqual(manifest["source_commit"], COMMIT)
            self.assertEqual(digest, hashlib.sha256(archive.read_bytes()).hexdigest())

    def test_wrong_source_version_and_dirty_tree_are_rejected(self):
        for values in ({"commit": "b" * 40}, {"version": "0.2.0-alpha.103"}, {"dirty": True}):
            with self.subTest(values=values), tempfile.TemporaryDirectory() as directory:
                archive, checksum = package(Path(directory), **values)
                with self.assertRaisesRegex(RuntimeError, "provenance"):
                    publisher["verify_package"](archive, checksum, VERSION, COMMIT)

    def test_changed_contents_are_rejected_even_with_a_new_outer_checksum(self):
        for values in ({"tamper": True}, {"extra": True}):
            with self.subTest(values=values), tempfile.TemporaryDirectory() as directory:
                archive, checksum = package(Path(directory), **values)
                with self.assertRaises(RuntimeError):
                    publisher["verify_package"](archive, checksum, VERSION, COMMIT)

    def test_failed_windows_runtime_cannot_publish(self):
        def require(condition, message):
            if not condition:
                raise RuntimeError(message)
        run = Mock(side_effect=[json.dumps({"head_sha": COMMIT, "head_branch": "v" + VERSION,
            "event": "push", "path": ".github/workflows/windows-client.yml"}), json.dumps({"jobs": [
                {"name": "cross-build", "conclusion": "success"},
                {"name": "windows-runtime", "conclusion": "failure"}]})])
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "GITHUB_REF_TYPE": "tag", "GITHUB_RUN_ID": "1"}):
            with self.assertRaisesRegex(RuntimeError, "runtime validation must pass"):
                publisher["publish"]("fixture/kite", "v" + VERSION, VERSION, COMMIT,
                    Path("notes"), Path("unused"), run=run, require=require,
                    release_info=Mock(), wait_for_created_release=Mock())
        self.assertEqual(run.call_count, 2)


if __name__ == "__main__":
    unittest.main()
