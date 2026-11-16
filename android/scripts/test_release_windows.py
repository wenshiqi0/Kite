"""Verify Windows provenance and release staging without running a Windows executable."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("release_alpha", Path(__file__).with_name("release-alpha.py"))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


class WindowsPublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.artifact = self.root / "artifact"
        self.artifact.mkdir()
        self.output = self.root / "release"
        self.output.mkdir()
        self.head = "a" * 40
        self.version = "0.2.1"

    def package(self, metadata=None, extra=False, tampered=False):
        files = {name: name.encode() for name in ("Kite.exe", "kite_ffi.dll", "kite_windows_media.dll")}
        manifest = {"source_commit": self.head, "version": self.version, "dirty": False,
                    "target": "win-x64", "built_on": "linux", "host_role": False,
                    "sha256": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
        manifest.update(metadata or {})
        archive = self.artifact / "Kite-windows-x64.zip"
        with zipfile.ZipFile(archive, "w") as bundle:
            bundle.writestr("Kite/build-manifest.json", json.dumps(manifest))
            for name, data in files.items():
                bundle.writestr("Kite/" + name, data + b"changed" if tampered else data)
            if extra:
                bundle.writestr("Kite/unknown.exe", b"unverified")
        archive.with_suffix(".zip.sha256").write_text(f"{publisher.digest(archive)}  {archive.name}\n")
        return archive

    def test_versioned_release_keeps_the_tested_zip_bytes(self):
        archive = self.package()
        staged, metadata = publisher.include_windows(self.artifact, self.output, self.version, self.head)
        self.assertEqual(staged.name, "kite-0.2.1-windows-x64.zip")
        self.assertEqual(staged.read_bytes(), archive.read_bytes())
        self.assertEqual(metadata["sha256"], publisher.digest(archive))
        self.assertEqual(publisher.include_windows(self.artifact, self.output, self.version, self.head),
                         (staged, metadata))

    def test_wrong_source_version_or_dirty_build_cannot_be_published(self):
        for metadata in ({"source_commit": "b" * 40}, {"version": "0.2.0-alpha.103"},
                         {"dirty": True}, {"host_role": True}, {"built_on": "windows"}):
            self.package(metadata)
            with self.subTest(metadata=metadata), self.assertRaisesRegex(RuntimeError, "provenance"):
                publisher.include_windows(self.artifact, self.output, self.version, self.head)
        self.assertEqual(list(self.output.iterdir()), [])

    def test_unverified_or_changed_files_are_rejected(self):
        self.package(extra=True)
        with self.assertRaisesRegex(RuntimeError, "unverified files"):
            publisher.include_windows(self.artifact, self.output, self.version, self.head)
        self.package(tampered=True)
        with self.assertRaisesRegex(RuntimeError, "file checksum"):
            publisher.include_windows(self.artifact, self.output, self.version, self.head)
        self.package()
        (self.artifact / "Kite-windows-x64.zip.sha256").write_text("invalid")
        with self.assertRaisesRegex(RuntimeError, "ZIP checksum"):
            publisher.include_windows(self.artifact, self.output, self.version, self.head)


if __name__ == "__main__":
    unittest.main()
