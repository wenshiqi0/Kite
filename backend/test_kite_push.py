import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import kite_push as push


class PushTests(unittest.TestCase):
    def test_payload_contains_only_routing_metadata_and_high_priority_visible_event(self):
        event = dict(id="a"*64, session_id="session", kind="question", created_ms=12, title="private title", source="private source")
        payload = push.payload(dict(host_id="host", token="test-token"), event)
        self.assertEqual(payload["message"]["data"], dict(kite_notice_id="host/"+"a"*64, kite_session_id="session", kite_kind="question", kite_created_ms="12"))
        self.assertEqual(payload["message"]["android"]["priority"], "HIGH")
        self.assertNotIn("private", json.dumps(payload))
        with self.assertRaises(ValueError):push.payload(dict(host_id="../bad", token="test-token"), event)

    def test_expired_token_is_removed_but_transient_errors_request_retry_without_leaking_provider_response(self):
        job=dict(subscription=dict(project_id="kite-test",host_id="host",token="test-token"),events=[dict(id="a"*64,session_id="session",kind="completed",created_ms=1)])
        def error(code):
            return urllib.error.HTTPError("url", 404, "provider", {}, io.BytesIO(json.dumps(dict(error=dict(details=[dict(errorCode=code)],message="private-token"))).encode()))
        for code,expected in (("UNREGISTERED","unregistered"),("UNAVAILABLE","retry")):
            with patch.object(push,"private_json",return_value=dict(project_id="kite-test")),patch.object(push,"access_token",return_value="test-access"),patch.object(push,"post",side_effect=error(code)):
                result=push.deliver("unused","unused",job)
                self.assertEqual(result,dict(results=[dict(id="a"*64,status=expected)]))
                self.assertNotIn("private-token",json.dumps(result))

    def test_project_mismatch_cannot_send_and_credentials_must_be_private_regular_files(self):
        with patch.object(push,"private_json",return_value=dict(project_id="kite-test")):
            with self.assertRaisesRegex(ValueError,"project mismatch"):
                push.deliver("unused","unused",dict(subscription=dict(project_id="other-project")))
        with tempfile.TemporaryDirectory() as directory:
            file=Path(directory)/"credentials.json";file.write_text('{}');file.chmod(0o644)
            with self.assertRaisesRegex(ValueError,"private credential"):push.private_json(file)
            file.chmod(0o600);self.assertEqual(push.private_json(file),{})
            link=Path(directory)/"link";link.symlink_to(file)
            with self.assertRaises(OSError):push.private_json(link)

    def test_oauth_cache_reuses_only_matching_account_before_expiry(self):
        with patch.object(push,"private_json",return_value=dict(email="test@example.com",expires=10**12,token="cached")),patch.object(push,"post") as post:
            self.assertEqual(push.access_token(dict(client_email="test@example.com"),"unused"),"cached")
            post.assert_not_called()


if __name__ == "__main__":
    unittest.main()
