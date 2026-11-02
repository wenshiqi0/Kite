#!/usr/bin/env python3
"""Verify the real memory cgroup on disposable processes; no model or user unit."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import uuid
from unittest.mock import patch

parser = argparse.ArgumentParser()
parser.add_argument("--evidence", type=Path)
args = parser.parse_args()
source = Path(__file__).resolve().parents[3] / "backend/kite_resources.py"
sys.path.insert(0, str(source.parent))
spec = importlib.util.spec_from_file_location("quota_probe", source)
resources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resources)
unit = "kitequotaqa" + uuid.uuid4().hex[:12] + ".slice"
scope = None
child = None
try:
    child = subprocess.Popen(["python3", "-c", "import time;initial=bytearray(4*1024*1024);print('ready',flush=True);time.sleep(2);extra=bytearray(128*1024*1024);time.sleep(1)"],
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, start_new_session=True)
    assert child.stdout.readline().strip() == "ready"
    item = resources.proc(child.pid)
    scope = resources.scope_name("qa:p1", child.pid, item["started"])
    with tempfile.TemporaryDirectory(prefix="kite-resource-probe-") as directory, patch.object(resources.Path, "home", return_value=Path(directory)), patch.object(resources, "SLICE", unit):
        owner = dict(pane="qa:p1", pid=child.pid, start_ticks=item["started"], registry_managed=False)
        result = resources.configure(64 * 1024**2, 64 * 1024**2, "unused", adopt=False, extra=(owner, [item]))
        cgroup = resources.group(unit)
        assert cgroup is not None
        assert resources.proc(child.pid)["cgroup"].endswith("/" + scope)
        assert result["reserved_bytes"] > 4 * 1024**2
        assert result["reserved_bytes"] + int((cgroup / "memory.max").read_text()) == 64 * 1024**2
        status = child.wait(timeout=25)
        events = resources.pairs(cgroup / "memory.events")
        evidence = dict(status="PASS", requested_bytes=64 * 1024**2,
                        legacy_reserved_bytes=result["reserved_bytes"],
                        effective_kernel_limit_bytes=result["effective_limit_bytes"],
                        allocator_exit=status, oom_kills=events.get("oom_kill", 0),
                        kernel_limit_verified=status != 0 and events.get("oom_kill", 0) > 0)
        assert evidence["kernel_limit_verified"]
        if args.evidence:
            args.evidence.parent.mkdir(parents=True, exist_ok=True)
            args.evidence.write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps(evidence))
finally:
    if child is not None and child.poll() is None:
        child.terminate()
        child.wait(timeout=5)
    if scope:
        subprocess.run(["systemctl", "--user", "stop", scope], capture_output=True, timeout=8)
    subprocess.run(["systemctl", "--user", "stop", unit], capture_output=True, timeout=8)
