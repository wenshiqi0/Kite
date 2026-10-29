"""Publication regressions from real tagged CI failures; no network or writes."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("release_alpha", Path(__file__).with_name("release-alpha.py"))
release_alpha = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_alpha)


class DraftPublicationTests(unittest.TestCase):
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
