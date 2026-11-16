"""Release preparation uses isolated real Git repositories, without network access."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

from release_version import prerelease, version_key

spec = importlib.util.spec_from_file_location("prepare_release", Path(__file__).with_name("prepare-release.py"))
prepare_release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare_release)


class PrepareReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "source"
        self.root.mkdir()
        self.git("init", "--initial-branch=main")
        self.git("config", "user.name", "Release fixture")
        self.git("config", "user.email", "release-fixture@example.invalid")
        (self.root / "android").mkdir()
        self.properties = self.root / "android/version.properties"
        self.properties.write_text("versionName=0.2.0-alpha.103\nversionCode=105\n")
        self.git("add", ".")
        self.git("commit", "-m", "source")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True,
                              capture_output=True, text=True).stdout.strip()

    def test_stable_preparation_binds_version_notes_commit_and_annotated_tag(self):
        notes = "# 0.2.1\n\nLiteral `$(no-shell)` and Chinese 中文.\n"
        before = self.git("rev-parse", "HEAD")
        result = prepare_release.prepare(self.root, "0.2.1", notes)
        self.assertEqual(result["versionCode"], 106)
        self.assertEqual(self.properties.read_text(), "versionName=0.2.1\nversionCode=106\n")
        self.assertEqual(self.git("rev-parse", "HEAD^"), before)
        self.assertEqual(self.git("rev-parse", "v0.2.1^{commit}"), result["commit"])
        self.assertEqual(self.git("cat-file", "-t", "v0.2.1"), "tag")
        self.assertEqual(self.git("for-each-ref", "--format=%(contents)", "refs/tags/v0.2.1"),
                         "<!-- Kite-Release-Targets: all -->\n\n" + notes.strip())
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_alpha_to_stable_order_and_future_alphas(self):
        self.assertLess(version_key("0.2.0-alpha.103"), version_key("0.2.0"))
        self.assertLess(version_key("0.2.1"), version_key("0.2.2-alpha.1"))
        self.assertTrue(prerelease("0.2.2-alpha.1"))
        self.assertFalse(prerelease("0.2.1"))
        self.assertEqual(prepare_release.prepare(self.root, "0.2.0-alpha.104", "")["versionCode"], 106)

    def test_invalid_or_reused_versions_leave_the_source_unchanged(self):
        before = self.git("rev-parse", "HEAD")
        for version in ("../0.2.1", "v0.2.1", "0.2.1;touch /tmp/no", "0.2.1-beta.1",
                        "0.2.0-alpha.103", "0.1.9", "0.02.1"):
            with self.subTest(version=version), self.assertRaises(RuntimeError):
                prepare_release.prepare(self.root, version, "")
        self.git("tag", "v0.2.1")
        with self.assertRaisesRegex(RuntimeError, "tag already exists"):
            prepare_release.prepare(self.root, "0.2.1", "")
        self.assertEqual(self.git("rev-parse", "HEAD"), before)
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_dirty_or_non_main_sources_are_not_prepared(self):
        self.properties.write_text(self.properties.read_text() + "# unfinished\n")
        with self.assertRaisesRegex(RuntimeError, "Commit source"):
            prepare_release.prepare(self.root, "0.2.1", "")
        self.git("checkout", "-b", "feature")
        with self.assertRaisesRegex(RuntimeError, "from main"):
            prepare_release.prepare(self.root, "0.2.1", "")

    def test_atomic_push_cannot_publish_a_tag_if_main_has_moved(self):
        remote = Path(self.temporary.name) / "remote.git"
        subprocess.run(["git", "init", "--bare", str(remote)], check=True, capture_output=True)
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "origin", "main")
        prepare_release.prepare(self.root, "0.2.1", "")
        peer = Path(self.temporary.name) / "peer"
        subprocess.run(["git", "clone", "--branch", "main", str(remote), str(peer)],
                       check=True, capture_output=True)
        def peer_git(*args):
            return subprocess.run(["git", *args], cwd=peer, check=True, capture_output=True)
        peer_git("config", "user.name", "Concurrent fixture")
        peer_git("config", "user.email", "concurrent@example.invalid")
        (peer / "new.txt").write_text("concurrent source")
        peer_git("add", ".")
        peer_git("commit", "-m", "concurrent update")
        peer_git("push", "origin", "main")
        with self.assertRaises(subprocess.CalledProcessError):
            self.git("push", "--atomic", "origin", "HEAD:refs/heads/main", "refs/tags/v0.2.1")
        self.assertEqual(self.git("ls-remote", "origin", "refs/tags/v0.2.1"), "")


if __name__ == "__main__":
    unittest.main()
