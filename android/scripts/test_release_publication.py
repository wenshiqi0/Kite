"""Exercise stable and Alpha publication end to end with local bytes and mocked GitHub."""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release_alpha", Path(__file__).with_name("release-alpha.py"))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


class PublicationTests(unittest.TestCase):
    def publish(self, version, expected_prerelease, existing=None, windows=False, recovery=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True,
                                      capture_output=True, text=True).stdout.strip()
            git("init", "--initial-branch=main")
            git("config", "user.name", "Release fixture")
            git("config", "user.email", "release-fixture@example.invalid")
            git("remote", "add", "origin", "https://github.com/fixture/kite.git")
            (root / "android").mkdir()
            (root / "android/version.properties").write_text(f"versionName={version}\nversionCode=106\n")
            notes = root / "docs/release-notes" / (version + ".md")
            notes.parent.mkdir(parents=True)
            notes.write_text("Release fixture notes\n")
            (root / ".gitignore").write_text("android/release-output/\n")
            git("add", ".")
            git("commit", "-m", "release source")
            git("tag", "-a", "v" + version, "-F", str(notes))
            head = git("rev-parse", "HEAD")
            output = root / "android/release-output" / ("v" + version)
            output.mkdir(parents=True)
            primary = output / f"kite-{version}-arm64-v8a.apk"
            compatible = output / f"kite-{version}-arm64-v8a-compat-v2.apk"
            for apk in (primary, compatible):
                with zipfile.ZipFile(apk, "w") as archive:
                    archive.writestr("classes.dex", b"same application")
                    archive.writestr("META-INF/CERT.SF", apk.name.encode())
            host = output / f"kite-host-{version}-linux-x86_64.tar.gz"
            host.write_bytes(b"verified host build")
            manifest_file = output / "release-manifest.json"
            manifest_file.write_text(json.dumps({
                "sourceCommit": head, "versionName": version, "versionCode": 106,
                "sha256": publisher.digest(primary), "variant": "release", "debuggable": False,
                "signingCertificateSha256": "f" * 64,
                "compatibilityApk": {"asset": compatible.name, "sha256": publisher.digest(compatible),
                    "signingCertificateSha256": "f" * 64, "signingSchemes": [1, 2],
                    "android16VerifiedScheme": 2},
                "linuxHostNode": {"asset": host.name, "sha256": publisher.digest(host)},
            }))
            sums = output / "SHA256SUMS"
            sums.write_text("".join(f"{publisher.digest(file)}  {file.name}\n"
                                    for file in (primary, compatible, host, manifest_file)))
            arguments = ["release-alpha.py", "--publish-only"]
            if recovery:
                arguments += ["--source-dir", str(root), "--verified-run-id", "17"]
            if windows:
                artifact = output.parent / "windows-artifact"
                artifact.mkdir()
                files = {name: name.encode() for name in ("Kite.exe", "kite_ffi.dll", "kite_windows_media.dll")}
                files["Kite.deps.json"] = json.dumps({"libraries": {"Kite/" + version: {}}}).encode()
                windows_manifest = {"source_commit": head, "version": version, "dirty": False,
                    "target": "win-x64", "built_on": "linux", "host_role": False,
                    "sha256": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
                archive = artifact / "Kite-windows-x64.zip"
                with zipfile.ZipFile(archive, "w") as bundle:
                    bundle.writestr("Kite/build-manifest.json", json.dumps(windows_manifest))
                    for name, data in files.items():
                        bundle.writestr("Kite/" + name, data)
                archive.with_suffix(".zip.sha256").write_text(f"{publisher.digest(archive)}  {archive.name}\n")
                arguments += ["--windows-artifact-dir", str(artifact)]
            draft = {"draft": True, "prerelease": expected_prerelease, "assets": []}
            public = {"draft": False, "prerelease": expected_prerelease,
                      "html_url": "https://github.com/fixture/kite/releases/tag/v" + version}
            calls = []
            original = publisher.run
            def run(*args, **kwargs):
                if args[0] != "gh":
                    return original(*args, **kwargs)
                calls.append(args)
                if args[1] == "api":
                    if "/jobs?" in args[2]:
                        return json.dumps({"jobs": [{"name": name, "conclusion": "success"} for name in (
                            "Build Release APK", "Build and verify Windows client / cross-build",
                            "Build and verify Windows client / windows-runtime")]})
                    return json.dumps({"head_sha": head, "head_branch": "v" + version,
                        "path": ".github/workflows/android-release.yml", "event": "push", "status": "completed"})
                if args[1:3] == ("release", "download"):
                    name = args[args.index("--pattern") + 1]
                    shutil.copyfile(output / name, Path(args[args.index("--dir") + 1]) / name)
                return ""
            environment = {"GITHUB_ACTIONS": "true", "GITHUB_REF_TYPE": "tag",
                           "GITHUB_REF_NAME": "v" + version}
            if recovery:
                environment.update({"GITHUB_REF_TYPE": "branch", "GITHUB_REF_NAME": "main", "GITHUB_SHA": "c" * 40})
            with patch.object(publisher, "ROOT", root), patch.object(publisher, "run", side_effect=run), \
                 patch.object(publisher, "release_info", side_effect=[existing, public]), \
                 patch.object(publisher, "wait_for_created_release", return_value=draft), \
                 patch.dict(os.environ, environment), \
                 patch.object(sys, "argv", arguments):
                publisher.main()
            if recovery:
                manifest = json.loads(manifest_file.read_text())
                self.assertEqual(manifest["sourceCommit"], head)
                self.assertEqual(manifest["publicationRecovery"], {"buildRun": 17, "toolsCommit": "c" * 40})
            return calls

    def test_stable_release_is_public_and_latest_only_after_download_verification(self):
        calls = self.publish("0.2.1", False)
        create = next(call for call in calls if call[1:3] == ("release", "create"))
        edit = next(call for call in calls if call[1:3] == ("release", "edit"))
        self.assertIn("--prerelease=false", create)
        self.assertIn("--verify-tag", create)
        self.assertNotIn("--target", create)
        self.assertNotIn("(Alpha)", create[create.index("--title") + 1])
        self.assertIn("--prerelease=false", edit)
        self.assertIn("--latest=true", edit)
        downloads = [index for index, call in enumerate(calls) if call[1:3] == ("release", "download")]
        self.assertEqual(len(downloads), 5)
        self.assertGreater(calls.index(edit), max(downloads))

    def test_alpha_release_remains_prerelease_and_never_latest(self):
        calls = self.publish("0.2.2-alpha.1", True)
        edit = next(call for call in calls if call[1:3] == ("release", "edit"))
        self.assertIn("--prerelease=true", edit)
        self.assertIn("--latest=false", edit)

    def test_stable_release_includes_all_six_assets_after_windows_verification(self):
        calls = self.publish("0.2.1", False, windows=True)
        downloads = [call for call in calls if call[1:3] == ("release", "download")]
        self.assertEqual(len(downloads), 6)
        self.assertIn("kite-0.2.1-windows-x64.zip", [call[call.index("--pattern") + 1] for call in downloads])

    def test_recovery_can_publish_the_original_tag_bytes_from_verified_builds(self):
        calls = self.publish("0.2.1", False, windows=True, recovery=True)
        downloads = [call for call in calls if call[1:3] == ("release", "download")]
        self.assertEqual(len(downloads), 6)
        self.assertTrue(any(call[1] == "api" and "/actions/runs/17/jobs?" in call[2] for call in calls))

    def test_recovery_rejects_unrelated_sources_and_failed_platform_checks(self):
        head, tag = "a" * 40, "v0.2.1"
        metadata = {"head_sha": head, "head_branch": tag, "path": ".github/workflows/android-release.yml",
                    "event": "push", "status": "completed"}
        jobs = {"jobs": [{"name": name, "conclusion": "success"} for name in (
            "Build Release APK", "Build and verify Windows client / cross-build",
            "Build and verify Windows client / windows-runtime")]}
        with patch.object(publisher, "run", side_effect=[json.dumps(metadata), json.dumps(jobs)]):
            self.assertEqual(publisher.verify_build_run("fixture/kite", "17", tag, head), metadata)
        for wrong in ({"head_sha": "b" * 40}, {"head_branch": "v0.2.2"}, {"status": "in_progress"},
                      {"path": ".github/workflows/other.yml"}):
            with patch.object(publisher, "run", return_value=json.dumps(metadata | wrong)):
                with self.subTest(wrong=wrong), self.assertRaisesRegex(RuntimeError, "does not match"):
                    publisher.verify_build_run("fixture/kite", "17", tag, head)
        jobs["jobs"][-1]["conclusion"] = "failure"
        with patch.object(publisher, "run", side_effect=[json.dumps(metadata), json.dumps(jobs)]):
            with self.assertRaisesRegex(RuntimeError, "runtime validation must pass"):
                publisher.verify_build_run("fixture/kite", "17", tag, head)

    def test_an_existing_release_cannot_change_its_stability(self):
        with self.assertRaisesRegex(RuntimeError, "different stable/prerelease"):
            self.publish("0.2.1", False, {"draft": True, "prerelease": True, "assets": []})


if __name__ == "__main__":
    unittest.main()
