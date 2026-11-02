#!/usr/bin/env python3
"""Linux resource facts and cgroup mechanisms. Admission policy belongs to Rust."""
import argparse
from contextlib import contextmanager
import fcntl
import functools
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

SLICE = "kite-agents.slice"
MAX_LEDGER = 1024 * 1024
MIN_CGROUP = 16 * 1024 * 1024


@functools.lru_cache(maxsize=1)
def codex_module():
    path = Path(os.environ.get("KITE_CODEX_BACKEND_PATH", str(Path(__file__).with_name("kite_codex.py"))))
    spec = importlib.util.spec_from_file_location("kite_resource_codex", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def root():
    path = Path.home() / ".local/state/kite/resources"
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink() or path.stat().st_uid != os.getuid():
        raise RuntimeError("invalid resource state directory")
    path.chmod(0o700)
    return path


@contextmanager
def locked():
    with (root() / "budget.lock").open("a") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        yield


def atomic(path, value):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(json.dumps(value, separators=(",", ":")).encode())
        stream.flush()
        os.fsync(stream.fileno())
    temporary.chmod(0o600)
    temporary.replace(path)


def ledger():
    path = root() / "budget.json"
    if not path.exists():
        return dict(version=1, legacy={}, units={})
    if path.stat().st_size > MAX_LEDGER:
        raise RuntimeError("resource ledger too large")
    value = json.loads(path.read_text())
    if value.get("version") != 1 or not isinstance(value.get("legacy"), dict) or not isinstance(value.get("units"), dict):
        raise RuntimeError("invalid resource ledger; original file preserved")
    if len(value["legacy"]) > 4096 or len(value["units"]) > 1024:
        raise RuntimeError("resource ledger capacity exceeded")
    for key, item in value["legacy"].items():
        if not isinstance(item.get("pid"), int) or item["pid"] < 2 or not str(item.get("started", "")).isdigit() or not isinstance(item.get("reserved"), int) or item["reserved"] < 0 or key != str(item["pid"]) + ":" + item["started"]:
            raise RuntimeError("invalid legacy memory reservation")
    for unit, item in value["units"].items():
        if unit != scope_name(item["pane"], item["pid"], item["started"]):
            raise RuntimeError("invalid resource scope identity")
    return value


def run(args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=8)
    if result.returncode:
        # Never include an environment or a full process command in diagnostics.
        raise RuntimeError("resource control command failed: " + result.stderr.strip()[:400])
    return result.stdout.strip()


def bus(method, signature, *args):
    return run(["busctl", "--user", "--timeout=5", "call", "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager",
                method, signature, *map(str, args)])


def group(unit):
    result = subprocess.run(["systemctl", "--user", "show", unit, "-p", "ControlGroup", "-p", "LoadState"],
                            capture_output=True, text=True, timeout=8)
    values = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
    if values.get("LoadState") == "not-found":
        return None
    if result.returncode:
        raise RuntimeError("resource unit inspection failed")
    value = values.get("ControlGroup", "")
    if not value:
        return None
    path = Path("/sys/fs/cgroup") / value.lstrip("/")
    if ".." in Path(value).parts or not path.is_dir():
        raise RuntimeError("invalid control group")
    return path


def proc(pid):
    directory = Path("/proc") / str(pid)
    fields = directory.joinpath("stat").read_text().rsplit(") ", 1)[1].split()
    status = {}
    for line in directory.joinpath("status").read_text().splitlines():
        key, _, value = line.partition(":")
        status[key] = value.strip()
    memory = lambda key: int(status.get(key, "0").split()[0]) * 1024
    return dict(pid=int(pid), ppid=int(fields[1]), started=fields[19],
                uid=int(status["Uid"].split()[0]), name=status["Name"][:64],
                ticks=int(fields[11]) + int(fields[12]), rss=memory("VmRSS"),
                reserved=memory("VmRSS") + memory("VmSwap") + memory("VmPTE") + 262144,
                cgroup=directory.joinpath("cgroup").read_text().strip().removeprefix("0::"))


def processes():
    result = {}
    for directory in Path("/proc").iterdir():
        if not directory.name.isdigit():
            continue
        try:
            value = proc(directory.name)
            result[value["pid"]] = value
        except (OSError, ValueError, KeyError, ProcessLookupError):
            pass
    return result


def live_legacy(value, table):
    result = {}
    for key, item in value["legacy"].items():
        current = table.get(item["pid"])
        if current and current["started"] == item["started"]:
            result[key] = item
    return result


def scope_name(pane, pid, started):
    return f"kite-agent-{int(pid)}-{int(started)}.scope"


def inside_budget(value):
    return "/" + SLICE + "/" in value["cgroup"]


def descendants(table, roots):
    found = set(roots)
    while True:
        children = {pid for pid, item in table.items() if item["ppid"] in found}
        extra = children - found
        if not extra:
            return [table[pid] for pid in found if pid in table]
        found.update(extra)


def managed(binary, table):
    codex = codex_module()
    result = []
    records = codex.catalog_read()["sessions"]
    claude = codex.api(binary, "agent.list", {})["agents"] if any(r.get("provider") == "claude" and r.get("reference") for r in records) else []
    for record in records:
        pane = record.get("pane_id")
        if record.get("provider") == "claude" and pane:
            if not any(codex.same_reference(agent, record) for agent in claude):
                continue
            process = codex.api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
            item = table.get(process["shell_pid"])
            if item:
                owner = dict(pane=pane, pid=item["pid"], start_ticks=item["started"], shell_pid=item["pid"], registry_managed=False)
                result.append((owner, descendants(table, [item["pid"]])))
            continue
        if record.get("provider") in ("dsh", "ply"):
            try:
                import kite_harness
                reference = record["reference"]
                if reference.get("source") != "kite:harness" or reference.get("agent") != record["provider"]:
                    continue
                directory = kite_harness.directory(reference["value"])
                state = json.loads((directory / "state.json").read_text())
                info = json.loads((directory / "session.json").read_text())
                item = table.get(state["pid"])
                if item is None or item["started"] != state["start_ticks"] or info["id"] != record["id"] or info.get("provider") != record["provider"]:
                    continue
                argv = Path("/proc", str(item["pid"]), "cmdline").read_bytes().decode().split("\0")
                if not any(Path(arg).name == "kite_harness_runtime.py" for arg in argv) or reference["value"] not in argv:
                    continue
                owner = dict(pane="kite-harness-" + reference["value"],pid=item["pid"],start_ticks=item["started"],registry_managed=False)
                result.append((owner, descendants(table, [item["pid"]])))
            except (OSError, ValueError, KeyError, RuntimeError):
                pass
            continue
        if record.get("provider") != "codex" or not pane:
            continue
        try:
            owner = codex.read_registry(pane)
            process = codex.api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
            owner = dict(owner, shell_pid=process["shell_pid"])
            items = descendants(table, [owner["pid"], process["shell_pid"]])
            result.append((owner, items))
        except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError):
            continue
    return result


def attach(unit, items):
    pids = []
    for item in items:
        try:
            current = proc(item["pid"])
            if current["uid"] == os.getuid() and current["started"] == item["started"]:
                pids.append(item["pid"])
        except (OSError, ProcessLookupError):
            pass
    if not pids:
        return
    existing = group(unit)
    if existing:
        if "/" + SLICE + "/" not in str(existing) or existing.name != unit:
            raise RuntimeError("resource scope identity mismatch")
        bus("AttachProcessesToUnit", "ssau", unit, "", len(pids), *pids)
    else:
        bus("StartTransientUnit", "ssa(sv)a(sa(sv))", unit, "fail", 4,
            "Description", "s", "Kite managed agent", "Slice", "s", SLICE,
            "CollectMode", "s", "inactive-or-failed", "PIDs", "au", len(pids), *pids, 0)
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        pending = []
        for pid in pids:
            try:
                if not proc(pid)["cgroup"].endswith("/" + unit):
                    pending.append(pid)
            except (OSError, ProcessLookupError):
                pass
        if not pending:
            return
        time.sleep(.02)
    raise RuntimeError("resource scope adoption was not confirmed")


def configure(limit, high, binary, adopt=True, extra=None):
    """Translate an aggregate budget into cgroup limits, including old charges.

    Linux keeps pre-migration memory charged to its original cgroup. Reserve
    that resident+swapped allocation conservatively until those processes exit.
    New agent runtimes are placed in their scope before exec.
    """
    codex = codex_module()
    with locked():
        value = ledger()
        table = processes()
        value["legacy"] = live_legacy(value, table)
        owners = managed(binary, table) if adopt else []
        if extra:
            owners.append(extra)
        manager = run(["systemctl", "--user", "show", "-p", "ControlGroup", "--value"])
        if not manager or manager == "/":
            raise RuntimeError("user resource delegation unavailable")
        for owner, items in owners:
            for item in items:
                if item["uid"] != os.getuid() or not item["cgroup"].startswith(manager + "/"):
                    raise RuntimeError("process is outside the managed user hierarchy")
                if not inside_budget(item) and item["pid"] != owner.get("bootstrap_pid"):
                    key = str(item["pid"]) + ":" + item["started"]
                    value["legacy"].setdefault(key, {k: item[k] for k in ("pid", "started", "reserved")})
        reserved = sum(item["reserved"] for item in value["legacy"].values())
        maximum = limit - reserved
        if maximum < MIN_CGROUP:
            raise RuntimeError("agent memory budget is below existing allocations")
        existing = group(SLICE)
        current = int(existing.joinpath("memory.current").read_text()) if existing else 0
        if maximum < current + MIN_CGROUP:
            raise RuntimeError("agent memory budget cannot shrink below current use")
        # Persist reservations before moving any process into the new hierarchy.
        value.update(limit_bytes=limit, high_bytes=high)
        atomic(root() / "budget.json", value)
        run(["systemctl", "--user", "set-property", "--runtime", SLICE,
             "MemoryAccounting=yes", f"MemoryMax={maximum}",
             f"MemoryHigh={max(MIN_CGROUP, min(maximum, high - reserved))}", "MemorySwapMax=0"])
        run(["systemctl", "--user", "start", SLICE])
        path = group(SLICE)
        if path is None or path.joinpath("memory.max").read_text().strip() != str(maximum):
            raise RuntimeError("kernel memory limit was not applied")
        for owner, items in owners:
            unit = scope_name(owner["pane"], owner["pid"], owner["start_ticks"])
            old_groups = {item["cgroup"] for item in items if item["pid"] in (owner["pid"], owner.get("shell_pid"))}
            stable = all(path.endswith("/" + unit) for path in old_groups)
            targets = items if owner.get("bootstrap_pid") else [item for item in items if not inside_budget(item) or not stable and item["cgroup"] in old_groups]
            attach(unit, targets)
            value["units"][unit] = dict(pane=owner["pane"], pid=owner["pid"],
                                         started=owner["start_ticks"], created_ms=int(time.time() * 1000))
            if not owner.get("bootstrap_pid") and owner.get("registry_managed", True):
                registry = codex.registry_path(owner["pane"])
                if codex.read_registry(owner["pane"]).get("budget_unit") != unit:
                    with codex.file_lock(codex.state_dir() / (codex.pane_key(owner["pane"]) + ".lock")):
                        current_owner = codex.read_registry(owner["pane"])
                        if (current_owner["pid"], current_owner["start_ticks"]) != (owner["pid"], owner["start_ticks"]):
                            raise RuntimeError("agent owner changed during resource adoption")
                        current_owner["budget_unit"] = unit
                        codex.atomic_json(registry, current_owner)
        atomic(root() / "budget.json", value)
        return dict(applied=True, reserved_bytes=reserved, effective_limit_bytes=maximum)


def kill_scope(unit):
    path = group(unit)
    if not path:
        return
    if not unit.startswith("kite-agent-") or not unit.endswith(".scope") or "/" + SLICE + "/" not in str(path):
        raise RuntimeError("unmanaged resource scope")
    bus("KillUnit", "ssi", unit, "all", int(signal.SIGTERM))
    deadline = time.monotonic() + 3
    def populated():
        try:
            return bool(path.joinpath("cgroup.procs").read_text().strip())
        except FileNotFoundError:
            return False
    while populated() and time.monotonic() < deadline:
        time.sleep(.05)
    if populated():
        bus("KillUnit", "ssi", unit, "all", int(signal.SIGKILL))


def stop_owner(owner):
    codex = codex_module()
    current = codex.read_registry(owner["pane"])
    unit = scope_name(owner["pane"], owner["pid"], owner["start_ticks"])
    if current != owner or owner.get("budget_unit") != unit or not proc(owner["pid"])["cgroup"].endswith("/" + unit):
        raise RuntimeError("resource owner identity changed")
    kill_scope(unit)


def reap_orphans():
    codex = codex_module()
    with locked():
        value = ledger()
        table = processes()
        records = codex.catalog_read()["sessions"]
        for unit, owner in list(value["units"].items()):
            current = table.get(owner["pid"])
            if current and current["started"] == owner["started"]:
                continue
            leases = []
            try:
                # A cold restore may be reusing this pane's shell. Never close
                # it while session-open is holding the durable session lease.
                for record in records:
                    if record.get("pane_id") == owner["pane"]:
                        lease = (codex.catalog_root() / ("session-" + record["id"] + ".lock")).open("a")
                        leases.append(lease)
                        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
                try:
                    replacement = codex.read_registry(owner["pane"])
                except (OSError, ValueError, KeyError, RuntimeError):
                    replacement = None
                if replacement and replacement["pid"] != owner["pid"] and replacement.get("budget_unit") in (None, unit):
                    continue
                kill_scope(unit)
                del value["units"][unit]
            except BlockingIOError:
                continue
            finally:
                for lease in leases:
                    lease.close()
        value["legacy"] = live_legacy(value, table)
        atomic(root() / "budget.json", value)


def integer(path, default=0):
    try:
        return int(path.read_text().strip())
    except (OSError, ValueError):
        return default


def pairs(path):
    try:
        return {key: int(number) for key, number in (line.split() for line in path.read_text().splitlines())}
    except (OSError, ValueError):
        return {}


def sample():
    mem = {line.split(":", 1)[0]: int(line.split()[1]) * 1024 for line in Path("/proc/meminfo").read_text().splitlines()}
    cpu = [int(value) for value in Path("/proc/stat").read_text().splitlines()[0].split()[1:9]]
    pressure = {}
    for line in Path("/proc/pressure/memory").read_text().splitlines():
        name, *values = line.split()
        pressure[name] = {key: float(number) for key, number in (item.split("=") for item in values)}
    table = processes()
    value = ledger()
    reserved = sum(item["reserved"] for item in live_legacy(value, table).values())
    uncontained = set()
    for info in value["units"].values():
        item = table.get(info["pid"])
        if item and item["started"] == info["started"] and not inside_budget(item):
            uncontained.add(item["pid"])
    codex = codex_module()
    for file in codex.state_dir().glob("*.json"):
        try:
            info = json.loads(file.read_text());item = table.get(info.get("pid"))
            if item and item["started"] == info.get("start_ticks") and not inside_budget(item):
                uncontained.add(item["pid"])
        except (OSError, ValueError, AttributeError):
            pass
    # Read the kernel's user hierarchy directly; do not execute systemctl per tick.
    manager = next((item["cgroup"].split("/app.slice/", 1)[0] for item in table.values()
                    if item["pid"] == os.getpid() and "/app.slice/" in item["cgroup"]), None)
    if manager is None:
        manager = run(["systemctl", "--user", "show", "-p", "ControlGroup", "--value"])
    budget = Path("/sys/fs/cgroup") / manager.lstrip("/") / "kite.slice" / SLICE
    events = pairs(budget / "memory.events")
    stats = pairs(budget / "cpu.stat")
    memory_stats = pairs(budget / "memory.stat")
    members = [item for item in table.values() if inside_budget(item)]
    top = sorted(table.values(), key=lambda item: item["rss"], reverse=True)[:8]
    return dict(sampled_at_ms=int(time.time() * 1000), cpu_total_ticks=sum(cpu),
                cpu_idle_ticks=cpu[3] + cpu[4], cpu_cores=os.cpu_count() or 1,
                load_average=list(os.getloadavg()), memory_total_bytes=mem["MemTotal"],
                memory_available_bytes=mem["MemAvailable"], swap_total_bytes=mem["SwapTotal"],
                swap_used_bytes=mem["SwapTotal"] - mem["SwapFree"],
                memory_pressure_some=pressure.get("some", {}).get("avg10", 0.0),
                memory_pressure_full=pressure.get("full", {}).get("avg10", 0.0),
                agent_current_bytes=integer(budget / "memory.current"),
                agent_reclaimable_bytes=min(integer(budget / "memory.current"), memory_stats.get("inactive_file", 0)),
                agent_reserved_bytes=reserved, agent_effective_limit_bytes=integer(budget / "memory.max"),
                agent_cpu_usage_usec=stats.get("usage_usec", 0), agent_process_count=len(members),
                agent_oom_kills=events.get("oom_kill", 0), agent_high_events=events.get("high", 0),
                uncontained_owners=len(uncontained),
                enforced=budget.exists() and integer(budget / "memory.max") > 0,
                processes=[dict(pid=item["pid"], name=item["name"], started=item["started"],
                                cpu_ticks=item["ticks"], memory_bytes=item["rss"],
                                managed=inside_budget(item)) for item in top])


def node(*args):
    executable = Path.home() / ".local/bin/kite-node"
    return json.loads(run([str(executable), *map(str, args)]))


def exec_agent(pane, binary, command):
    codex = codex_module()
    pid = os.getpid()
    admission = node("resource-admit", "--pane", pane, "--pid", pid)
    if not admission.get("allowed"):
        raise RuntimeError("agent resource admission denied: " + str(admission.get("reason", "unavailable")))
    process = codex.api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
    if process["foreground_process_group_id"] != process["shell_pid"]:
        raise RuntimeError("agent pane is not an available shell")
    table = processes()
    owner = dict(pane=pane, pid=pid, start_ticks=table[pid]["started"], bootstrap_pid=pid, shell_pid=process["shell_pid"])
    configure(int(admission["limit_bytes"]), int(admission["high_bytes"]), binary,
              adopt=False, extra=(owner, descendants(table, [pid, process["shell_pid"]])))
    os.execvpe(command[0], command, os.environ)


def exec_harness(pane, command):
    pid = os.getpid()
    admission = node("resource-admit", "--pane", pane, "--pid", pid)
    if not admission.get("allowed"):
        raise RuntimeError("agent resource admission denied: " + str(admission.get("reason", "unavailable")))
    table = processes()
    owner = dict(pane=pane, pid=pid, start_ticks=table[pid]["started"], bootstrap_pid=pid, registry_managed=False)
    configure(int(admission["limit_bytes"]), int(admission["high_bytes"]), None,
              adopt=False, extra=(owner, descendants(table, [pid])))
    os.execvpe(command[0], command, os.environ)


def prepare_pane(pane, binary):
    codex = codex_module()
    process = codex.api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
    if process["foreground_process_group_id"] != process["shell_pid"]:
        raise RuntimeError("agent pane is not an available shell")
    pid = process["shell_pid"]
    admission = node("resource-admit", "--pane", pane, "--pid", pid)
    if not admission.get("allowed"):
        raise RuntimeError("agent resource admission denied: " + str(admission.get("reason", "unavailable")))
    table = processes()
    owner = dict(pane=pane, pid=pid, start_ticks=table[pid]["started"], registry_managed=False)
    configure(int(admission["limit_bytes"]), int(admission["high_bytes"]), binary,
              adopt=False, extra=(owner, descendants(table, [pid])))
    return dict(pid=pid, budget_unit=scope_name(pane, pid, owner["start_ticks"]))


def ready(owner):
    if owner.get("budget_unit"):
        try:
            node("resource-ready", "--pid", owner["pid"])
        except (OSError, RuntimeError, subprocess.SubprocessError):
            # A conservative startup reservation expires if the node is upgrading.
            pass


def reserve_existing(owner):
    # Reattaching an existing TUI is free, but recreating its client can load
    # a large context. Reuse the owner startup reservation when one exists.
    if os.environ.get("KITE_AGENT_RESOURCE_CONTROL") != "1":
        return
    admission = node("resource-admit", "--pane", owner["pane"], "--pid", owner["pid"])
    if not admission.get("allowed"):
        raise RuntimeError("agent resource admission denied: " + str(admission.get("reason", "unavailable")))


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("sample", "configure", "exec-agent", "exec-harness", "orphans"))
    parser.add_argument("--pane")
    parser.add_argument("--herdr")
    args, command = parser.parse_known_args()
    if command and args.action not in ("exec-agent", "exec-harness"):
        raise ValueError("unexpected resource arguments")
    if args.action == "sample":
        print(json.dumps(sample()))
    elif args.action == "configure":
        data = sys.stdin.buffer.read(65537)
        if len(data) > 65536:
            raise ValueError("resource request too large")
        request = json.loads(data)
        print(json.dumps(configure(int(request["limit_bytes"]), int(request["high_bytes"]), args.herdr)))
    elif args.action == "orphans":
        reap_orphans()
        print("{}")
    else:
        if command and command[0] == "--":
            command = command[1:]
        if not command:
            raise ValueError("agent command required")
        if args.action == "exec-harness":
            exec_harness(args.pane, command)
        else:
            exec_agent(args.pane, args.herdr, command)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(type(error).__name__ + ": " + str(error), file=sys.stderr)
        sys.exit(1)
