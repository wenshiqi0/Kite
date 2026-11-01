import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import kite_resources as resources


class ResourceTests(unittest.TestCase):
    def test_debt_stays_reserved_until_the_original_process_exits(self):
        value = dict(legacy={"42:7": dict(pid=42, started="7", reserved=1234)})
        self.assertEqual(sum(x["reserved"] for x in resources.live_legacy(value, {42: dict(started="7")}).values()), 1234)
        self.assertEqual(resources.live_legacy(value, {42: dict(started="8")}), {})
        self.assertEqual(resources.live_legacy(value, {}), {})

    def test_aliases_share_one_scope_without_repeated_migration(self):
        self.assertEqual(resources.scope_name("w1:p1", 42, "7"), resources.scope_name("w2:p1", 42, "7"))
        self.assertNotEqual(resources.scope_name("w1:p1", 42, "7"), resources.scope_name("w1:p1", 42, "8"))

    def test_scope_requires_the_actual_budget_parent(self):
        self.assertTrue(resources.inside_budget(dict(cgroup="/user/kite.slice/kite-agents.slice/kite-agent-42-7.scope")))
        self.assertFalse(resources.inside_budget(dict(cgroup="/user/app.slice/kite-node.service")))
        self.assertFalse(resources.inside_budget(dict(cgroup="/user/kite-agents.slice-unrelated/unit")))

    def test_descendants_include_orphan_safe_identity_records_only_from_known_roots(self):
        table = {1: dict(pid=1, ppid=0), 2: dict(pid=2, ppid=1), 3: dict(pid=3, ppid=2), 4: dict(pid=4, ppid=1)}
        self.assertEqual({x["pid"] for x in resources.descendants(table, [2])}, {2, 3})

    def test_invalid_ledger_is_not_replaced_with_an_empty_budget(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(resources.Path, "home", return_value=Path(directory)):
            path = resources.root() / "budget.json"
            path.write_text("broken")
            with self.assertRaises(ValueError): resources.ledger()
            self.assertEqual(path.read_text(), "broken")
            path.write_text(json.dumps(dict(version=1, legacy={"42:7": dict(pid=42, started="7", reserved=-100)}, units={})))
            with self.assertRaisesRegex(RuntimeError, "reservation"): resources.ledger()

    def test_unknown_unit_is_absent_but_control_failures_are_reported(self):
        from types import SimpleNamespace
        with patch.object(resources.subprocess, "run", return_value=SimpleNamespace(returncode=4, stdout="LoadState=not-found\n", stderr="")):
            self.assertIsNone(resources.group("kite-agent-42-7.scope"))
        with patch.object(resources.subprocess, "run", return_value=SimpleNamespace(returncode=1, stdout="", stderr="private")):
            with self.assertRaises(RuntimeError): resources.group("kite-agent-42-7.scope")


if __name__ == "__main__": unittest.main()
