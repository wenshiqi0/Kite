"""Opt-in discovery, real subprocess protocols and durable chat ownership contracts."""
import json
import os
from pathlib import Path
import signal
import socket
import sys
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

import kite_harness as harness
import kite_codex as backend

FIXTURE = Path(__file__).parent / "fixtures/fake_harness.py"


class Models(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200 if self.headers.get("Authorization") == "Bearer fixture-only" else 401)
        self.end_headers()
        self.wfile.write(b'{"data":[{"id":"deepseek-v4-flash"}]}')

    def log_message(self, *_args):
        pass


class HarnessTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="kh-")
        self.addCleanup(self.temporary.cleanup)
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Models)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        endpoint = f"http://127.0.0.1:{self.server.server_port}/models"
        environment = {"KITE_HARNESS_STATE_DIR": str(Path(self.temporary.name) / "harnesses"), "DEEPSEEK_API_KEY": "fixture-only",
            "KITE_ENABLE_DSH": "0", "KITE_AGENT_RESOURCE_CONTROL": "0",
            "KITE_DSH_COMMAND": json.dumps([sys.executable, str(FIXTURE), "dsh"]),
            "KITE_DSH_HEALTH_URL": endpoint}
        self.environment = patch.dict(os.environ, environment)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.addCleanup(self.stop_owners)

    def stop_owners(self):
        for path in harness.root().glob("*/state.json"):
            pid = json.loads(path.read_text()).get("pid")
            if pid:
                try:
                    os.kill(pid, signal.SIGTERM)
                    os.waitpid(pid, 0)
                except (ProcessLookupError, ChildProcessError):
                    pass

    def enable(self, provider):
        os.environ["KITE_ENABLE_" + provider.upper()] = "1"

    def record(self, provider):
        return {"id": ("a" if provider == "dsh" else "b") * 64, "provider": provider,
                "cwd": self.temporary.name, "title": provider, "reference": None}

    def open(self, provider):
        self.enable(provider)
        record = self.record(provider)
        agent = harness.open_session(record)
        record["reference"] = agent["agent_session"]
        return record, agent

    def rpc(self, thread, method, params):
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(5)
            connection.connect(str(harness.directory(thread) / "owner.sock"))
            connection.sendall((json.dumps({"id": "test", "method": method, "params": params}) + "\n").encode())
            with connection.makefile("rb") as source:
                while line := source.readline():
                    value = json.loads(line)
                    if value.get("id") == "test":
                        return value
        self.fail("owner closed without a response")

    def submit(self, thread, prompt):
        value = self.rpc(thread, "turn/start", {"threadId": thread,
            "model": "deepseek-v4-flash", "input": [{"type": "text", "text": prompt}]})
        self.assertIn("result", value)
        path = harness.directory(thread)
        for _ in range(100):
            if json.loads((path / "state.json").read_text())["status"] != "working":
                return [json.loads(line) for line in (path / "history.jsonl").read_text().splitlines()]
            time.sleep(.05)
        self.fail("turn did not settle")

    def test_disabled_or_missing_credentials_are_not_advertised(self):
        self.assertNotIn("dsh", [p["id"] for p in harness.providers()])
        self.enable("dsh")
        os.environ["DEEPSEEK_API_KEY"] = ""
        self.assertNotIn("dsh", [p["id"] for p in harness.providers()])

    def test_only_authenticated_healthy_native_backends_are_advertised(self):
        self.enable("dsh")
        ids = [p["id"] for p in harness.providers()]
        self.assertIn("dsh", ids)
        self.assertNotIn("fixture-only", json.dumps(harness.providers()))
        os.environ["DEEPSEEK_API_KEY"] = "invalid"
        self.assertNotIn("dsh", [p["id"] for p in harness.providers()])

    def test_wrong_native_identity_and_missing_binary_stay_hidden(self):
        self.enable("dsh")
        os.environ["KITE_DSH_COMMAND"] = json.dumps([sys.executable, str(FIXTURE), "wrong"])
        self.assertNotIn("dsh", [p["id"] for p in harness.providers()])
        os.environ["KITE_DSH_COMMAND"] = '["/nonexistent/kite-owned-dsh"]'
        self.assertNotIn("dsh", [p["id"] for p in harness.providers()])

    def test_dsh_sdk_turns_tools_and_reconnection_share_one_owner(self):
        record, agent = self.open("dsh")
        thread = agent["agent_session"]["value"]
        rows = self.submit(thread, "first owned prompt")
        self.assertTrue(any(r["payload"].get("name") == "read" for r in rows))
        self.assertTrue(any("DSH reply 1" in json.dumps(r) for r in rows))
        pid = json.loads((harness.directory(thread) / "state.json").read_text())["pid"]
        again = harness.open_session(record)
        self.assertEqual(again["agent_session"], agent["agent_session"])
        self.assertEqual(json.loads((harness.directory(thread) / "state.json").read_text())["pid"], pid)
        rows = self.submit(thread, "second owned prompt")
        self.assertTrue(any("DSH reply 2" in json.dumps(r) for r in rows))
        self.assertEqual(len([r for r in rows if r["payload"].get("role") == "user"]), 2)

    def test_cross_session_and_unsupported_inputs_are_rejected(self):
        _, agent = self.open("dsh")
        thread = agent["agent_session"]["value"]
        self.assertIn("error", self.rpc(thread, "thread/resume", {"threadId": "another"}))
        self.assertIn("error", self.rpc(thread, "turn/start", {"threadId": thread,
            "input": [{"type": "localImage", "path": "/not-owned"}]}))
        self.assertFalse((harness.directory(thread) / "history.jsonl").exists())

    def test_scan_keeps_saved_identity_and_excludes_other_catalogs(self):
        record, agent = self.open("dsh")
        self.assertEqual(harness.agents([record])[0]["terminal_id"], agent["terminal_id"])
        self.assertEqual(harness.agents([]), [])

    def test_completion_facts_use_real_turn_identity_without_message_bodies(self):
        record, agent = self.open("dsh")
        record["archived"] = False
        thread = agent["agent_session"]["value"]
        self.submit(thread, "owned-private-prompt")
        with patch.object(backend, "api", return_value={"agents": []}), patch.object(backend, "catalog_read", return_value={"sessions": [record]}):
            facts = json.loads(backend.notification_facts("fixture"))["facts"]
        self.assertEqual(len(facts), 1)
        self.assertTrue(facts[0]["lifecycle"])
        control = json.loads(facts[0]["controls"])
        self.assertEqual(control["payload"]["type"], "task_complete")
        self.assertEqual(control["payload"]["turn_id"], json.loads((harness.directory(thread)/"state.json").read_text())["turn_id"])
        self.assertNotIn("owned-private-prompt", json.dumps(facts))
        self.assertNotIn("DSH reply", json.dumps(facts))
