import contextlib
import copy
import hashlib
import json
import os
from pathlib import Path
import signal
import tempfile
import unittest
from unittest.mock import patch

import kite_codex as backend


class LifecycleTests(unittest.TestCase):
    def setUp(self):
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.home = Path(self.stack.enter_context(tempfile.TemporaryDirectory()))
        self.stack.enter_context(patch.object(backend.Path, "home", return_value=self.home))
        self.stack.enter_context(patch.dict(os.environ, {"CODEX_HOME": str(self.home / ".codex")}))
        self.thread = "00000000-0000-0000-0000-000000000001"
        self.reference = dict(agent="codex", kind="id", source="herdr:codex", value=self.thread)
        self.record = dict(id="a" * 32, provider="codex", reference=self.reference,
                           pane_id="w1:p1", terminal_id="term1", workspace_id="w1",
                           archived=True, cwd=str(self.home))
        backend.catalog_write(dict(backend.catalog_read(), sessions=[self.record]))
        self.owner = dict(protocol=1, pane="w1:p1", pid=424242, start_ticks="1",
                          socket="/private/owner.sock", last_open_ms=1000)
        self.registry = backend.registry_path("w1:p1")
        backend.atomic_json(self.registry, self.owner)
        self.history = self.home / ".codex/sessions" / ("rollout-test-" + self.thread + ".jsonl")
        self.history.parent.mkdir(parents=True)
        self.history.write_text('{"type":"response_item","payload":{"text":"preserved context"}}\n')
        os.utime(self.history, ns=(100_000_000_000, 100_000_000_000))
        self.real_read_registry = backend.read_registry
        self.stack.enter_context(patch.object(backend, "read_registry", return_value=self.owner))
        self.stack.enter_context(patch.object(backend, "process_stopped", return_value=False))
        self.agent = dict(pane_id="w1:p1", agent_session=self.reference,
                          agent_status="done", interactive_ready=True)
        self.process = dict(shell_pid=41, foreground_process_group_id=43,
                            foreground_processes=[dict(name="codex", pid=43,
                                argv=["codex", "--remote", "unix://" + self.owner["socket"]])])
        self.stack.enter_context(patch.object(backend, "api", side_effect=self.api))
        self.statuses = {self.thread: "idle"}
        self.stack.enter_context(patch.object(backend, "backend_rpc", side_effect=self.rpc))

    def api(self, binary, method, params):
        if method == "agent.list":
            return dict(agents=[self.agent] if self.agent else [])
        if method == "pane.process_info":
            return dict(process_info=self.process)
        raise AssertionError(method)

    def rpc(self, pane, method, params, **kwargs):
        if method == "thread/loaded/list":
            return dict(data=list(self.statuses))
        if method == "thread/read":
            thread = params["threadId"]
            return dict(thread=dict(id=thread, status=dict(type=self.statuses[thread])))
        raise AssertionError(method)

    def fact(self):
        return backend.lifecycle_fact(self.record, "herdr")

    def reap_fixture(self, fact=None):
        fact = copy.deepcopy(fact or self.fact())
        expected = dict(fact, reason="archived")
        self.stack.enter_context(patch.object(backend, "lifecycle_fact", return_value=fact))
        self.stack.enter_context(patch.object(backend, "start_ticks", return_value="1"))
        self.stack.enter_context(patch.object(backend, "process_live", return_value=False))
        self.stack.enter_context(patch.object(backend.os, "getpgid", return_value=self.owner["pid"]))
        kill = self.stack.enter_context(patch.object(backend.os, "killpg"))
        quit_tui = self.stack.enter_context(patch.object(backend.os, "kill"))
        return expected, kill, quit_tui

    def test_discovery_and_keepalive_do_not_extend_activity(self):
        before = self.registry.read_bytes()
        first = self.fact()
        with patch.object(backend.time, "time", return_value=999999):
            second = self.fact()
        self.assertTrue(first["idle"] and first["persisted"])
        self.assertEqual(first["activity"], second["activity"])
        self.assertEqual(first["last_activity_ms"], 100000)
        self.assertEqual(second["last_activity_ms"], 100000)
        self.assertEqual(self.registry.read_bytes(), before)
        with patch.object(backend.time, "time", return_value=123):
            backend.note_session_open("w1:p1")
        self.assertEqual(json.loads(self.registry.read_text())["last_open_ms"], 123000)

    def test_new_provider_output_is_activity(self):
        first = self.fact()
        self.history.write_text(self.history.read_text() + '{"type":"event_msg"}\n')
        second = self.fact()
        self.assertNotEqual(first["activity"], second["activity"])
        self.assertGreater(second["last_activity_ms"], first["last_activity_ms"])

    def test_continue_of_an_unmanaged_terminal_does_not_create_an_owner(self):
        self.registry.unlink()
        with patch.object(backend, "read_registry") as read:
            backend.note_session_open("w1:p1")
        read.assert_not_called()
        self.assertFalse(self.registry.exists())

    def test_active_blocked_and_background_threads_are_preserved(self):
        for state in ("active", "notLoaded", "unknown"):
            self.statuses[self.thread] = state
            self.assertFalse(self.fact()["idle"])
        self.statuses[self.thread] = "idle"
        self.statuses["00000000-0000-0000-0000-000000000002"] = "active"
        self.assertFalse(self.fact()["idle"])
        self.statuses.pop("00000000-0000-0000-0000-000000000002")
        self.agent["agent_status"] = "blocked"
        self.assertFalse(self.fact()["idle"])

    def test_paused_terminal_is_not_given_a_pending_exit(self):
        with patch.object(backend, "process_stopped", return_value=True):
            self.assertFalse(self.fact()["idle"])
        expected, kill, stop_tui = self.reap_fixture()
        with patch.object(backend, "process_stopped", return_value=True):
            self.assertEqual(backend.lifecycle_reap(expected, "herdr")["reason"], "tui_paused")
        kill.assert_not_called()
        stop_tui.assert_not_called()

    def test_unsaved_and_mismatched_threads_are_not_reapable(self):
        self.history.unlink()
        self.assertFalse(self.fact()["persisted"])
        self.record["reference"] = dict(self.reference, value="00000000-0000-0000-0000-000000000099")
        self.assertFalse(self.fact()["idle"])

    def test_quit_is_followed_by_owner_shutdown_without_deleting_context(self):
        catalog = (self.home / ".local/share/kite/sessions.json").read_bytes()
        history = self.history.read_bytes()
        expected, kill, quit_tui = self.reap_fixture()
        result = backend.lifecycle_reap(expected, "herdr")
        self.assertTrue(result["reclaimed"])
        quit_tui.assert_called_once()
        quit_tui.assert_called_once_with(43, signal.SIGTERM)
        kill.assert_called_once_with(self.owner["pid"], signal.SIGTERM)
        self.assertFalse(self.registry.exists())
        self.assertEqual(self.history.read_bytes(), history)
        self.assertEqual((self.home / ".local/share/kite/sessions.json").read_bytes(), catalog)

    def test_new_activity_and_changed_process_identity_cancel_reaping(self):
        for field, changed in (("activity", "new-output"), ("owner", dict(self.owner, pid=777))):
            with self.subTest(field=field):
                expected, kill, quit_tui = self.reap_fixture()
                expected[field] = changed
                self.assertFalse(backend.lifecycle_reap(expected, "herdr")["reclaimed"])
                kill.assert_not_called()
                quit_tui.assert_not_called()

    def test_activity_after_tui_disconnect_preserves_the_owner(self):
        expected, kill, _ = self.reap_fixture()
        after = dict(expected, activity="new-output", tui_pid=None)
        with patch.object(backend, "lifecycle_fact", side_effect=[expected, after]):
            self.assertFalse(backend.lifecycle_reap(expected, "herdr")["reclaimed"])
        kill.assert_not_called()

    def test_reused_pid_is_rejected_before_signalling(self):
        with patch.object(backend, "start_ticks", return_value="different"), patch.object(backend.os, "killpg") as kill:
            with self.assertRaisesRegex(RuntimeError, "process changed"):
                self.real_read_registry("w1:p1")
            kill.assert_not_called()

    def test_unarchive_and_shared_owner_cancel_reaping(self):
        expected, kill, _ = self.reap_fixture()
        backend.patch_session(self.record["id"], dict(archived=False))
        self.assertFalse(backend.lifecycle_reap(expected, "herdr")["reclaimed"])
        backend.patch_session(self.record["id"], dict(archived=True))
        catalog = backend.catalog_read()
        catalog["sessions"].append(dict(self.record, id="b" * 32, pane_id="w2:p1", archived=False))
        backend.catalog_write(catalog)
        self.assertEqual(backend.lifecycle_reap(expected, "herdr")["reason"], "shared_owner")
        kill.assert_not_called()

    def test_cold_continue_uses_original_thread_and_context(self):
        self.registry.unlink()
        self.agent = None
        self.process.update(foreground_process_group_id=41, foreground_processes=[dict(name="bash", pid=41, argv=["bash"])])
        started = {}
        restored = dict(agent="codex", pane_id="w1:p1", workspace_id="w1", terminal_id="term1", agent_session=self.reference)
        def api(binary, method, params):
            if method == "agent.list":
                return dict(agents=[restored] if started else [])
            if method == "pane.process_info":
                return dict(process_info=self.process)
            if method == "agent.start":
                started.update(params)
                return dict(agent=restored)
            raise AssertionError(method)
        before = hashlib.sha256(self.history.read_bytes()).hexdigest()
        with patch.object(backend.shutil, "which", return_value="/fixture/codex"), patch.object(backend, "api", side_effect=api), patch.object(backend, "ensure", return_value=self.owner), patch.object(backend, "note_session_open"), patch.object(backend, "backend_rpc", return_value={}), patch.object(backend, "bind"):
            result = backend.session_open(self.record["id"], "herdr")
        self.assertEqual(result["agent"]["agent_session"], self.reference)
        self.assertEqual(started["args"][-2:], ["resume", self.thread])
        self.assertEqual(backend.catalog_read()["sessions"][0]["reference"], self.reference)
        self.assertEqual(hashlib.sha256(self.history.read_bytes()).hexdigest(), before)


if __name__ == "__main__":
    unittest.main()
