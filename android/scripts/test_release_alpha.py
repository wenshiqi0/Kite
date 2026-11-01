"""Publication regressions from real tagged CI failures; no network or writes."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
import tempfile
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release_alpha", Path(__file__).with_name("release-alpha.py"))
release_alpha = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_alpha)


class DraftPublicationTests(unittest.TestCase):
    def test_explicit_resign_preserves_payload_and_uses_password_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); built = root / "built.apk"; output = root / "signed.apk"
            with zipfile.ZipFile(built, "w") as archive:
                archive.writestr("AndroidManifest.xml", b"unchanged application")
                archive.writestr("META-INF/CERT.SF", b"old signature")
            def align(*args):
                Path(args[-1]).write_bytes(built.read_bytes())
                return ""
            def sign(command, **kwargs):
                with zipfile.ZipFile(output, "w") as archive:
                    archive.writestr("AndroidManifest.xml", b"unchanged application")
                    archive.writestr("META-INF/CERT.SF", b"fresh signature")
                return subprocess.CompletedProcess(command, 0, "", "")
            signing = {"store.password": "test-store-password", "key.password": "test-key-password", "key.alias": "existing"}
            with patch.object(release_alpha, "run", side_effect=align), patch.object(release_alpha.subprocess, "run", side_effect=sign) as signed:
                release_alpha.resign_apk(built, output, Path("tools"), root / "stable.keystore", signing)
            command = signed.call_args.args[0]
            self.assertNotIn("test-store-password", command)
            self.assertNotIn("test-key-password", command)
            self.assertEqual(signed.call_args.kwargs["env"]["KITE_APK_STORE_PASSWORD"], "test-store-password")
            for version in (1, 2, 3):
                self.assertEqual(command[command.index(f"--v{version}-signing-enabled") + 1], "true")
            self.assertEqual(command[command.index("--v4-signing-enabled") + 1], "false")
            self.assertEqual(release_alpha.apk_payload(built), release_alpha.apk_payload(output))

    def test_payload_guard_includes_dependency_metadata_but_excludes_signatures(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = Path(directory) / "app.apk"
            with zipfile.ZipFile(apk, "w") as archive:
                archive.writestr("classes.dex", b"application")
                archive.writestr("META-INF/dependency.version", b"must be preserved")
                archive.writestr("META-INF/CERT.RSA", b"signature")
            self.assertEqual(set(release_alpha.apk_payload(apk)), {"classes.dex", "META-INF/dependency.version"})

    def test_verified_release_requires_all_signature_schemes_and_native_alignment(self):
        certificate = "f" * 64
        signature = "\n".join([f"Verified using v{v} scheme (test): {'false' if v == 1 else 'true'}" for v in (1, 2, 3)]) + f"\nSigner #1 certificate SHA-256 digest: {certificate}"
        legacy = f"Verified using v1 scheme (test): true\nSigner #1 certificate SHA-256 digest: {certificate}"
        badging = "package: name='io.github.wenshiqi0.kite' versionCode='12' versionName='0.2.0-alpha.10'\nnative-code: 'arm64-v8a'"
        with patch.object(release_alpha, "run", side_effect=[badging, signature, legacy, signature, ""]) as call:
            abis, actual = release_alpha.inspect_apk(Path("app.apk"), Path("tools"), "0.2.0-alpha.10", 12)
        self.assertEqual(abis, ["arm64-v8a"])
        self.assertEqual(actual, certificate)
        self.assertEqual(call.call_args.args, ("tools/zipalign", "-c", "-P", "16", "4", "app.apk"))
        for missing in (1, 2, 3):
            modern = signature.replace(f"v{missing} scheme (test): true", f"v{missing} scheme (test): false")
            jar = legacy.replace("scheme (test): true", "scheme (test): false") if missing == 1 else legacy
            with self.subTest(missing=missing), patch.object(release_alpha, "run", side_effect=[badging, modern, jar]):
                with self.assertRaisesRegex(RuntimeError, f"signing scheme v{missing}"):
                    release_alpha.inspect_apk(Path("app.apk"), Path("tools"), "0.2.0-alpha.10", 12)
        with patch.object(release_alpha, "run", side_effect=[badging, signature, legacy.replace(certificate, "a" * 64)]):
            with self.assertRaisesRegex(RuntimeError, "same certificate"):
                release_alpha.inspect_apk(Path("app.apk"), Path("tools"), "0.2.0-alpha.10", 12)

    def test_compatibility_signing_keeps_v1_v2_and_explicitly_disables_v3(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); built = root / "built.apk"; output = root / "compat.apk"
            with zipfile.ZipFile(built, "w") as archive:
                archive.writestr("classes.dex", b"same application")
            def align(*args): Path(args[-1]).write_bytes(built.read_bytes()); return ""
            def sign(command, **kwargs):
                output.write_bytes(built.read_bytes())
                return subprocess.CompletedProcess(command, 0, "", "")
            signing = {"store.password":"private", "key.password":"private", "key.alias":"existing"}
            with patch.object(release_alpha, "run", side_effect=align), patch.object(release_alpha.subprocess, "run", side_effect=sign) as call:
                release_alpha.resign_apk(built, output, Path("tools"), root / "existing.keystore", signing, v3=False)
            command = call.call_args.args[0]
            self.assertEqual(command[command.index("--v3-signing-enabled")+1], "false")
            self.assertEqual(command[command.index("--v2-signing-enabled")+1], "true")
            self.assertEqual(command[command.index("--v1-signing-enabled")+1], "true")
            self.assertEqual(release_alpha.apk_payload(built), release_alpha.apk_payload(output))

    def test_compatibility_package_requires_v2_on_android16_and_the_same_certificate(self):
        certificate = "f"*64
        modern = f"Verified using v2 scheme (test): true\nVerified using v3 scheme (test): false\nSigner #1 certificate SHA-256 digest: {certificate}"
        legacy = f"Verified using v1 scheme (test): true\nSigner #1 certificate SHA-256 digest: {certificate}"
        badging = "package: name='io.github.wenshiqi0.kite' versionCode='35' versionName='0.2.0-alpha.33'\nnative-code: 'arm64-v8a'"
        with patch.object(release_alpha, "run", side_effect=[badging, modern, legacy, modern, ""]) as call:
            release_alpha.inspect_apk(Path("compat.apk"), Path("tools"), "0.2.0-alpha.33", 35, v3=False)
        self.assertIn("36", call.call_args_list[-2].args)
        with patch.object(release_alpha, "run", side_effect=[badging, modern, legacy, modern.replace("v2 scheme (test): true", "v2 scheme (test): false")]):
            with self.assertRaisesRegex(RuntimeError,"Android 16 must verify"):
                release_alpha.inspect_apk(Path("compat.apk"), Path("tools"), "0.2.0-alpha.33", 35, v3=False)
        with patch.object(release_alpha, "run", side_effect=[badging, modern.replace("v3 scheme (test): false", "v3 scheme (test): true")]):
            with self.assertRaisesRegex(RuntimeError,"avoid the v3"):
                release_alpha.inspect_apk(Path("compat.apk"), Path("tools"), "0.2.0-alpha.33", 35, v3=False)

    def test_tag_404_recovers_existing_draft_from_release_list(self):
        result = subprocess.CompletedProcess([], 1, "", "HTTP 404: Not Found")
        with patch.object(release_alpha.subprocess, "run", return_value=result), patch.object(
            release_alpha, "run", return_value='[[{"tag_name":"v0.2.0-alpha.2","draft":true,"prerelease":true}]]'
        ) as read:
            draft = release_alpha.release_info("owner/repo", "v0.2.0-alpha.2")
        self.assertTrue(draft["draft"])
        self.assertIn("--paginate", read.call_args.args)

    def test_new_draft_visibility_retries_reads_only(self):
        draft = {"draft": True, "prerelease": True}
        with patch.object(release_alpha, "release_info", side_effect=[None, None, draft]) as read, patch("time.sleep") as sleep:
            self.assertEqual(release_alpha.wait_for_created_release("owner/repo", "tag"), draft)
        self.assertEqual(read.call_count, 3)
        self.assertEqual([c.args[0] for c in sleep.call_args_list], [1, 2])

    def test_missing_draft_has_bounded_wait_and_recovery_error(self):
        with patch.object(release_alpha, "release_info", return_value=None) as read, patch("time.sleep"):
            with self.assertRaisesRegex(RuntimeError, "rerun the publish job"):
                release_alpha.wait_for_created_release("owner/repo", "tag")
        self.assertEqual(read.call_count, 5)


if __name__ == "__main__":
    unittest.main()
