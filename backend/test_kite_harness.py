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
        self.wfile.write(b'{"data":[{"id":"deepseek-v4-flash"},{"id":"deepseek-flash"}]}')

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
            "KITE_ENABLE_DSH": "0", "KITE_ENABLE_PLY": "0", "KITE_AGENT_RESOURCE_CONTROL": "0",
            "KITE_DSH_MODEL": "deepseek-v4-flash", "KITE_DSH_PROVIDER": "deepseek-official",
            "KITE_PLY_MODEL": "deepseek-v4-flash", "KITE_PLY_PROVIDER": "deepseek",
            "KITE_PLY_COMMAND": json.dumps([sys.executable, str(FIXTURE), "ply"]),
            "KITE_PLY_HEALTH_URL": endpoint,
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

            "model": json.dumps(["deepseek-official", "deepseek-v4-flash"], separators=(",", ":")), "effort": "high", "input": [{"type": "text", "text": prompt}]})
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

    def test_dsh_acp_turns_tools_and_reconnection_share_one_owner(self):
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
        self.assertNotIn("PRIVATE REASONING", json.dumps(rows))

    def test_release_then_cold_restore_retains_dsh_and_ply_native_context(self):
        for provider in ("dsh", "ply"):
            with self.subTest(provider=provider):
                record, agent = self.open(provider)
                thread = agent["agent_session"]["value"]
                path = harness.directory(thread)
                catalog = self.rpc(thread, "model/list", {})["result"]["data"]
                response = self.rpc(thread, "turn/start", {"threadId": thread, "model": catalog[0]["model"],
                    "effort": catalog[0]["defaultReasoningEffort"], "input": [{"type": "text", "text": "owned context before release"}]})
                self.assertIn("result", response)
                for _ in range(100):
                    state = json.loads((path / "state.json").read_text())
                    if state["status"] == "done": break
                    time.sleep(.02)
                self.assertEqual(state["status"], "done")
                native = (path / "native.json").read_bytes()
                history = (path / "history.jsonl").read_bytes()
                pid = state["pid"]
                os.kill(pid, signal.SIGTERM)
                try:
                    os.waitpid(pid, 0)
                except ChildProcessError:
                    # open_session's owned waiter may have reaped it first.
                    pass
                for _ in range(100):
                    if harness.start_ticks(pid) != state["start_ticks"]: break
                    time.sleep(.02)
                self.assertNotEqual(harness.start_ticks(pid), state["start_ticks"])
                restored = harness.open_session(record)
                self.assertEqual(restored["agent_session"], agent["agent_session"])
                self.assertNotEqual(json.loads((path / "state.json").read_text())["pid"], pid)
                self.assertEqual((path / "native.json").read_bytes(), native)
                self.assertEqual((path / "history.jsonl").read_bytes(), history)

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

    def test_ply_linux_host_contract_uses_the_shared_composer(self):
        _, agent = self.open("ply")
        thread = agent["agent_session"]["value"]
        catalog = self.rpc(thread, "model/list", {})["result"]["data"]
        self.assertEqual(catalog[0]["inputModalities"], ["text", "image"])
        self.assertEqual([e["reasoningEffort"] for e in catalog[0]["supportedReasoningEfforts"]], ["off", "high", "max"])
        value = self.rpc(thread, "turn/start", {"threadId": thread, "model": catalog[0]["model"], "effort": "max", "input": [{"type": "text", "text": "owned"}]})
        self.assertIn("result", value)
        path = harness.directory(thread)
        for _ in range(100):
            if json.loads((path / "state.json").read_text())["status"] == "done":
                break
            time.sleep(.02)
        self.assertIn("Ply reply max", (path / "history.jsonl").read_text())
        self.assertTrue((path / "native.json").exists())

    def test_dsh_model_effort_and_native_vision_are_independent(self):
        import base64
        import uuid
        _, agent = self.open("dsh")
        thread = agent["agent_session"]["value"]
        catalog = self.rpc(thread, "model/list", {})["result"]["data"]
        vision = next(row for row in catalog if "image" in row["inputModalities"])
        upload = Path.home() / ".local/share/kite/uploads" / uuid.uuid4().hex
        upload.mkdir(parents=True, mode=0o700)
        self.addCleanup(upload.rmdir)
        file = upload / "owned.png"
        file.write_bytes(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a9iUAAAAASUVORK5CYII="))
        self.addCleanup(file.unlink)
        value = self.rpc(thread, "turn/start", {"threadId": thread, "model": vision["model"], "effort": "max",
            "input": [{"type": "text", "text": "inspect image"}, {"type": "localImage", "path": str(file)}]})
        self.assertIn("result", value)
        path = harness.directory(thread)
        for _ in range(100):
            if json.loads((path / "state.json").read_text())["status"] != "working":
                break
            time.sleep(.03)
        rows = (path / "history.jsonl").read_text()
        self.assertIn("deepseek-flash max native-vision:image/png", rows)
        self.assertNotIn("iVBORw0", rows)
        self.assertNotIn("PRIVATE REASONING", rows)
        self.assertIn("error", self.rpc(thread, "turn/start", {"threadId": thread, "model": vision["model"], "effort": "made-up", "input": [{"type": "text", "text": "owned"}]}))

    def test_permissions_survive_client_disconnect_and_cancel_uses_native_turn(self):
        _, agent = self.open("dsh")
        thread = agent["agent_session"]["value"]
        model = self.rpc(thread, "model/list", {})["result"]["data"][0]["model"]
        result = self.rpc(thread, "turn/start", {"threadId": thread, "model": model, "effort": "low", "input": [{"type": "text", "text": "approval-fixture"}]})
        self.assertIn("result", result)
        path = harness.directory(thread)
        for _ in range(100):
            if json.loads((path / "state.json").read_text())["status"] == "blocked": break
            time.sleep(.02)
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(5)
            connection.connect(str(path / "owner.sock"))
            connection.sendall((json.dumps({"id": "resume", "method": "thread/resume", "params": {"threadId": thread}})+"\n").encode())
            with connection.makefile("rb") as source:
                request = json.loads(source.readline())
                self.assertEqual(request["method"], "item/commandExecution/requestApproval")
                connection.sendall((json.dumps({"id": request["id"], "result": {"decision": "accept"}})+"\n").encode())
        for _ in range(100):
            if json.loads((path / "state.json").read_text())["status"] == "done": break
            time.sleep(.02)
        self.assertEqual(json.loads((path / "state.json").read_text())["status"], "done")
        result = self.rpc(thread, "turn/start", {"threadId": thread, "model": model, "effort": "high", "input": [{"type": "text", "text": "cancel-fixture"}]})
        turn = result["result"]["turn"]["id"]
        self.assertIn("error", self.rpc(thread, "turn/interrupt", {"threadId": thread, "turnId": "expired"}))
        self.assertIn("result", self.rpc(thread, "turn/interrupt", {"threadId": thread, "turnId": turn}))
        for _ in range(100):
            if json.loads((path / "state.json").read_text())["status"] == "cancelled": break
            time.sleep(.02)
        self.assertEqual(json.loads((path / "state.json").read_text())["status"], "cancelled")

    def test_supplemental_input_is_accepted_in_order_while_turn_is_running(self):
        for provider in ("dsh", "ply"):
            _, agent = self.open(provider)
            thread = agent["agent_session"]["value"]
            model = self.rpc(thread, "model/list", {})["result"]["data"][0]["model"]
            first = self.rpc(thread, "turn/start", {"threadId": thread, "model": model, "effort": "high", "input": [{"type": "text", "text": "first owned input"}]})
            second = self.rpc(thread, "turn/start", {"threadId": thread, "model": model, "effort": "high", "input": [{"type": "text", "text": "supplemental owned input"}]})
            self.assertIn("result", first)
            self.assertIn("result", second)
            path = harness.directory(thread)
            for _ in range(100):
                if json.loads((path / "state.json").read_text())["status"] == "done": break
                time.sleep(.03)
            rows = [json.loads(line)["payload"] for line in (path / "history.jsonl").read_text().splitlines()]
            users = [r["content"][0]["text"] for r in rows if r.get("role") == "user"]
            self.assertEqual(users, ["first owned input", "supplemental owned input"])
            self.assertEqual(len([r for r in rows if r.get("role") == "assistant"]), 2)
            self.assertEqual(json.loads((path / "queued.json").read_text()), [])
