#!/usr/bin/env python3
"""Kite's per-pane Codex host adapter. No TCP listener or global daemon changes."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import socket
import stat
import subprocess
import sys
import time
import base64
import selectors
import struct
import select
from contextlib import contextmanager
import tempfile
import shutil


def atomic_json(path, value):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as file:
        file.write(json.dumps(value, ensure_ascii=False).encode())
        file.flush(); os.fsync(file.fileno())
        temporary = Path(file.name)
    temporary.chmod(0o600)
    temporary.replace(path)
    descriptor = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


@contextmanager
def file_lock(path):
    import fcntl
    with path.open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def catalog_root():
    return private_dir(Path.home() / ".local/share/kite")


def catalog_unlocked():
    path = catalog_root() / "sessions.json"
    value = json.loads(path.read_text()) if path.exists() else {"version": 1, "revision": 0, "sessions": []}
    if value.get("version") != 1:
        raise RuntimeError("unsupported session catalog")
    value["now_ms"] = int(time.time() * 1000)
    return value


def catalog_read():
    with file_lock(catalog_root() / "sessions.lock"):
        return catalog_unlocked()


def catalog_write(value):
    if len(value.get("sessions", [])) > 1024:
        raise ValueError("session capacity exceeded")
    with file_lock(catalog_root() / "sessions.lock"):
        current = catalog_unlocked()
        if current["revision"] != value["revision"]:
            return {"conflict": True}
        value.update(version=1, revision=current["revision"] + 1, now_ms=int(time.time() * 1000))
        atomic_json(catalog_root() / "sessions.json", value)
        return value


def patch_session(identifier, fields):
    with file_lock(catalog_root() / "sessions.lock"):
        value = catalog_unlocked()
        record = next(r for r in value["sessions"] if r["id"] == identifier)
        record.update(fields)
        value["revision"] += 1
        atomic_json(catalog_root() / "sessions.json", value)
        return record

PROTOCOL = 1
MAX_FILE = 25 * 1024 * 1024


def private_dir(path):
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid():
        raise RuntimeError("invalid private directory")
    path.chmod(0o700)
    return path


def state_dir():
    return private_dir(Path.home() / ".local/state/kite/codex")


def pane_key(pane):
    if not re.fullmatch(r"[A-Za-z0-9_-]+:p[0-9]+", pane):
        raise ValueError("invalid pane ID")
    return hashlib.sha256(pane.encode()).hexdigest()[:20]


def registry_path(pane):
    return state_dir() / (pane_key(pane) + ".json")


def start_ticks(pid):
    return Path(f"/proc/{pid}/stat").read_text().rsplit(") ", 1)[1].split()[19]


def read_registry(pane):
    info = json.loads(registry_path(pane).read_text())
    if info["protocol"] != PROTOCOL or info["pane"] != pane:
        raise RuntimeError("backend protocol mismatch")
    if start_ticks(info["pid"]) != info["start_ticks"]:
        raise RuntimeError("backend process changed")
    args = Path(f'/proc/{info["pid"]}/cmdline').read_bytes().split(b"\0")
    if b"app-server" not in args or ("unix://" + info["socket"]).encode() not in args:
        raise RuntimeError("backend identity mismatch")
    path = Path(info["socket"])
    # Codex may publish a symlink to its shortened, hashed control-socket path.
    link = path.lstat()
    st = path.stat()
    parent = path.resolve().parent.stat()
    if link.st_uid != os.getuid() or not stat.S_ISSOCK(st.st_mode) or st.st_uid != os.getuid() or st.st_mode & 0o077 or parent.st_uid != os.getuid() or parent.st_mode & 0o077:
        raise RuntimeError("backend socket is not private")
    return info


def api(binary, method, params):
    out = subprocess.run([binary, "remote-api-bridge"], input=json.dumps({"id": "kite-backend", "method": method, "params": params}) + "\n", text=True, capture_output=True, timeout=15, check=True)
    response = json.loads(out.stdout)
    if "error" in response:
        raise RuntimeError("herdr " + json.dumps(response["error"]))
    return response["result"]


def ensure(pane, binary):
    try:
        return read_registry(pane)
    except (OSError, ValueError, KeyError, RuntimeError):
        pass
    # Serialize separate phones without creating duplicate session owners.
    import fcntl
    with (state_dir() / (pane_key(pane) + ".lock")).open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            return read_registry(pane)
        except (OSError, ValueError, KeyError, RuntimeError):
            pass
        version = subprocess.check_output(["codex", "--version"], text=True, timeout=10).strip()
        match = re.search(r"(\d+)\.(\d+)\.(\d+)", version)
        if not match or tuple(map(int, match.groups())) < (0, 164, 0):
            raise RuntimeError("Codex 0.164.0 or newer is required")
        process = api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
        shell = process["shell_pid"]
        cwd = Path(f"/proc/{shell}/cwd").resolve(strict=True)
        env = os.environ.copy()
        env.pop("CODEX_THREAD_ID", None)
        shell_env = Path(f"/proc/{shell}/environ").read_bytes().split(b"\0")
        env.update(dict(x.decode().split("=", 1) for x in shell_env if x.startswith(b"HERDR_")))
        if env.get("HERDR_PANE_ID") != pane or not env.get("HERDR_SOCKET_PATH"):
            raise RuntimeError("pane environment unavailable")
        runtime = Path(f"/run/user/{os.getuid()}")
        root = private_dir((runtime if runtime.is_dir() else state_dir()) / "kite")
        sock = root / ("codex-" + pane_key(pane) + ".sock")
        # Never unlink a live socket. Stale sockets are removed by Codex itself.
        if sock.exists():
            with socket.socket(socket.AF_UNIX) as probe:
                probe.settimeout(1)
                if probe.connect_ex(str(sock)) == 0:
                    raise RuntimeError("unregistered live backend; inspect before retrying")
        log = state_dir() / (pane_key(pane) + ".log")
        descriptor = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        try:
            child = subprocess.Popen(["codex", "-c", "check_for_update_on_startup=false", "app-server", "--listen", "unix://" + str(sock)], stdin=subprocess.DEVNULL, stdout=descriptor, stderr=descriptor, env=env, cwd=cwd, start_new_session=True)
        finally:
            os.close(descriptor)
        info = {"protocol": PROTOCOL, "pane": pane, "socket": str(sock), "pid": child.pid, "start_ticks": start_ticks(child.pid), "last_open_ms": int(time.time() * 1000)}
        for _ in range(100):
            if child.poll() is not None:
                raise RuntimeError("Codex backend exited; inspect private host log")
            if sock.exists():
                sock.chmod(0o600)
                path = registry_path(pane)
                temporary = path.with_suffix(".tmp")
                temporary.write_text(json.dumps(info))
                temporary.chmod(0o600)
                temporary.replace(path)
                return read_registry(pane)
            time.sleep(.1)
        child.terminate()
        raise TimeoutError("Codex backend startup timed out")


def upgrade(pane, binary):
    agents = api(binary, "agent.list", {})["agents"]
    agent = next(a for a in agents if a["pane_id"] == pane)
    if agent.get("agent") != "codex" or agent.get("agent_status") not in ("idle", "done"):
        raise RuntimeError("only an idle Codex pane can be upgraded")
    thread = agent["agent_session"]
    if thread["kind"] != "id" or not re.fullmatch(r"[a-f0-9-]{36}", thread["value"]):
        raise RuntimeError("a persisted Codex thread ID is required")
    info = ensure(pane, binary)
    process = api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
    front = process["foreground_processes"]
    if len(front) != 1 or front[0]["name"] != "codex" or "--no-daemon" not in front[0]["argv"]:
        raise RuntimeError("expected one idle in-process Codex TUI; no process was stopped")
    pid = front[0]["pid"]
    ticks = start_ticks(pid)
    # Recheck state immediately before changing the session owner.
    fresh = next(a for a in api(binary, "agent.list", {})["agents"] if a["pane_id"] == pane)
    if fresh["agent_status"] not in ("idle", "done") or fresh["agent_session"] != thread or start_ticks(pid) != ticks:
        raise RuntimeError("session became active; upgrade cancelled")
    os.kill(pid, signal.SIGTERM)
    for _ in range(100):
        if not Path(f"/proc/{pid}").exists():
            break
        time.sleep(.1)
    else:
        raise RuntimeError("old session did not stop; inspect before retrying")
    time.sleep(.5)
    api(binary, "agent.start", {"pane_id": pane, "name": agent["name"], "kind": "codex", "args": ["--remote", "unix://" + info["socket"], "-c", "check_for_update_on_startup=false", "resume", thread["value"]]})
    for _ in range(100):
        try:
            bind(pane, binary, thread["value"])
        except (RuntimeError, OSError):
            pass
        fresh = next((a for a in api(binary, "agent.list", {})["agents"] if a["pane_id"] == pane), {})
        if fresh.get("agent_session", {}).get("value") == thread["value"] and fresh.get("interactive_ready"):
            return {"thread": thread["value"], "pane": pane, "protocol": PROTOCOL}
        time.sleep(.2)
    raise RuntimeError("TUI resume not confirmed; inspect pane, do not create another thread")


def backend_rpc(pane, method, params, timeout=10):
    process = subprocess.Popen([sys.executable, __file__, "proxy", "--pane", pane], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    buffer = bytearray()
    def rpc(identifier, method, params):
        process.stdin.write((json.dumps({"id": identifier, "method": method, "params": params}) + "\n").encode())
        process.stdin.flush()
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if b"\n" not in buffer:
                if not select.select([process.stdout], [], [], max(0, deadline - time.monotonic()))[0]:
                    break
                data = os.read(process.stdout.fileno(), 65536)
                if not data:
                    break
                buffer.extend(data)
                if len(buffer) > 2 * 1024 * 1024:
                    raise RuntimeError("oversized backend metadata")
            while b"\n" in buffer:
                index = buffer.index(10)
                result = json.loads(buffer[:index])
                del buffer[:index+1]
                if result.get("id") == identifier:
                    if "error" in result:
                        raise RuntimeError(str(result["error"].get("message", "backend metadata unavailable"))[:1024])
                    return result["result"]
        raise RuntimeError("backend metadata timed out")
    try:
        rpc(1, "initialize", {"clientInfo": {"name": "kite-host", "version": "1"}})
        process.stdin.write(b'{"method":"initialized"}\n'); process.stdin.flush()
        return rpc(2, method, params)
    finally:
        process.terminate()
        process.wait(timeout=5)


def bind(pane, binary, expected=None):
    threads = backend_rpc(pane, "thread/loaded/list", {})["data"]
    if expected is not None:
        if expected not in threads:
            raise RuntimeError("expected pane-owned Codex thread is not loaded")
        thread = expected
    elif len(threads) == 1:
        thread = threads[0]
    else:
        raise RuntimeError("expected exactly one pane-owned Codex thread")
    api(binary, "pane.report_agent_session", {"pane_id": pane, "source": "herdr:codex", "agent": "codex", "agent_session_id": thread, "resume_argv": ["python3", str(Path(__file__).resolve()), "tui", "--pane", pane, "--herdr", binary, "--thread", thread]})
    return {"thread": thread}


def same_reference(agent, record):
    reference = record.get("reference")
    return reference is not None and reference.get("agent") == record["provider"] and agent.get("agent_session") == reference


def provider_history_exists(reference):
    if reference["agent"] == "codex":
        if reference["kind"] != "id" or not re.fullmatch(r"[a-f0-9-]{36}", reference["value"]):
            raise ValueError("invalid Codex session reference")
        root = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")) / "sessions"
        return next(root.rglob("rollout-*-" + reference["value"] + ".jsonl"), None) is not None
    return True


def note_session_open(pane):
    """An explicit continue/restore is activity; discovery and polls are not."""
    if not registry_path(pane).exists():
        # Herdr can also expose a user-owned Codex TUI without a Kite owner.
        return
    with file_lock(state_dir() / (pane_key(pane) + ".lock")):
        info = read_registry(pane)
        info["last_open_ms"] = int(time.time() * 1000)
        atomic_json(registry_path(pane), info)


def lifecycle_fact(record, binary, agents=None):
    """Bounded local facts only. The host's Rust policy selects when to reap."""
    pane = record["pane_id"]
    owner = read_registry(pane)
    agents = agents if agents is not None else api(binary, "agent.list", {})["agents"]
    agent = next((a for a in agents if a["pane_id"] == pane), None)
    process = api(binary, "pane.process_info", {"pane_id": pane})["process_info"]
    front = process["foreground_processes"]
    shell = process["foreground_process_group_id"] == process["shell_pid"]
    remote_tui = (len(front) == 1 and front[0]["name"] == "codex" and
                  "--remote" in front[0].get("argv", []) and
                  "unix://" + owner["socket"] in front[0].get("argv", []))
    pane_idle = ((agent is None and shell) or
                 (agent is not None and same_reference(agent, record) and
                  agent.get("agent_status") in ("idle", "done") and
                  agent.get("interactive_ready") and remote_tui))
    loaded = backend_rpc(pane, "thread/loaded/list", {}, timeout=2)["data"]
    if len(loaded) > 64:
        raise ValueError("loaded thread capacity exceeded")
    statuses = []
    histories = []
    reference = record.get("reference") or {}
    root = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")) / "sessions"
    for thread in loaded:
        if not re.fullmatch(r"[a-f0-9-]{36}", thread):
            raise ValueError("invalid loaded thread identity")
        value = backend_rpc(pane, "thread/read", {"threadId": thread, "includeTurns": False}, timeout=2)["thread"]
        if value.get("id") != thread:
            raise RuntimeError("loaded thread identity changed")
        statuses.append(value.get("status", {}).get("type", "unknown"))
        files = list(root.rglob("rollout-*-" + thread + ".jsonl"))
        if len(files) == 1:
            info = files[0].stat()
            histories.append([thread, info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns])
    # Include activity in a saved but currently unloaded conversation too.
    if not loaded and reference.get("kind") == "id" and re.fullmatch(r"[a-f0-9-]{36}", reference.get("value", "")):
        files = list(root.rglob("rollout-*-" + reference["value"] + ".jsonl"))
        if len(files) == 1:
            info = files[0].stat()
            histories.append([reference["value"], info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns])
    boot = next(int(line.split()[1]) for line in Path("/proc/stat").read_text().splitlines() if line.startswith("btime "))
    started_ms = int((boot + int(owner["start_ticks"]) / os.sysconf("SC_CLK_TCK")) * 1000)
    last_open_ms = owner.get("last_open_ms", started_ms)
    activity = hashlib.sha256(json.dumps([last_open_ms, sorted(histories)]).encode()).hexdigest()
    return dict(session_id=record["id"], pane=pane, owner=owner,
                reference=record.get("reference"), archived=bool(record.get("archived")),
                idle=bool(pane_idle and all(s == "idle" for s in statuses) and
                          (not loaded or reference.get("value") in loaded)),
                persisted=len(histories) >= len(loaded), activity=activity,
                last_activity_ms=max([last_open_ms] + [h[4] // 1000000 for h in histories]),
                tui_pid=front[0]["pid"] if remote_tui else None)


def lifecycle_facts(binary):
    records = catalog_read()["sessions"]
    if len(records) > 1024:
        raise ValueError("session capacity exceeded")
    agents = api(binary, "agent.list", {})["agents"]
    facts = []
    for record in records:
        if record.get("provider") != "codex" or not record.get("pane_id"):
            continue
        try:
            facts.append(lifecycle_fact(record, binary, agents))
        except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError):
            # An unavailable owner or ambiguous state never authorizes a stop.
            continue
    return dict(facts=facts)


def process_live(pid, ticks):
    try:
        if start_ticks(pid) != str(ticks):
            return False
        return "State:\tZ" not in Path(f"/proc/{pid}/status").read_text()
    except (OSError, ProcessLookupError):
        return False


def lifecycle_reap(expected, binary):
    """Revalidate identity/activity under the same leases as session-open."""
    identifier = expected["session_id"]
    pane = expected["pane"]
    reason = expected["reason"]
    if not re.fullmatch(r"[a-f0-9]{32,64}", identifier) or reason not in ("archived", "idle_timeout"):
        raise ValueError("invalid lifecycle request")
    with file_lock(catalog_root() / ("session-" + identifier + ".lock")):
        with file_lock(state_dir() / (pane_key(pane) + ".lock")):
            catalog = catalog_read()
            record = next((r for r in catalog["sessions"] if r["id"] == identifier), None)
            if record is None or record.get("pane_id") != pane or record.get("provider") != "codex":
                return dict(reclaimed=False, reason="record_changed")
            if reason == "archived" and not record.get("archived"):
                return dict(reclaimed=False, reason="unarchived")
            fresh = lifecycle_fact(record, binary)
            for field in ("owner", "reference", "activity", "archived"):
                if fresh[field] != expected[field]:
                    return dict(reclaimed=False, reason="activity_changed")
            if not fresh["idle"] or not fresh["persisted"]:
                return dict(reclaimed=False, reason="not_idle_or_not_persisted")
            owner = fresh["owner"]
            # A restored pane can alias an existing owner. Do not kill a process
            # serving another durable record, even if this record is archived.
            for other in catalog["sessions"]:
                if other["id"] == identifier or not other.get("pane_id"):
                    continue
                try:
                    alias = read_registry(other["pane_id"])
                except (OSError, ValueError, KeyError, RuntimeError):
                    continue
                if (alias["pid"], alias["start_ticks"]) == (owner["pid"], owner["start_ticks"]):
                    return dict(reclaimed=False, reason="shared_owner")
            if fresh["tui_pid"] is not None:
                pid = fresh["tui_pid"]
                ticks = start_ticks(pid)
                subprocess.run([binary, "agent", "prompt", pane, "/quit"], check=True,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
                deadline = time.monotonic() + 4
                while process_live(pid, ticks) and time.monotonic() < deadline:
                    time.sleep(.05)
                if process_live(pid, ticks):
                    return dict(reclaimed=False, reason="tui_still_connected")
            # /quit disconnects a remote TUI; it does not stop its app-server.
            after = lifecycle_fact(record, binary)
            if any(after[k] != fresh[k] for k in ("owner", "reference", "activity")) or not after["idle"]:
                return dict(reclaimed=False, reason="activity_changed")
            if read_registry(pane) != owner or os.getpgid(owner["pid"]) != owner["pid"]:
                return dict(reclaimed=False, reason="owner_changed")
            os.killpg(owner["pid"], signal.SIGTERM)
            deadline = time.monotonic() + 4
            while process_live(owner["pid"], owner["start_ticks"]) and time.monotonic() < deadline:
                time.sleep(.05)
            if process_live(owner["pid"], owner["start_ticks"]):
                return dict(reclaimed=False, reason="owner_still_running")
            if json.loads(registry_path(pane).read_text()) == owner:
                registry_path(pane).unlink()
            return dict(reclaimed=True, session_id=identifier)


def session_open(identifier, binary):
    """Serialize reconnect/create for one durable record, never a model turn."""
    if not re.fullmatch(r"[a-f0-9]{32,64}", identifier):
        raise ValueError("invalid session ID")
    with file_lock(catalog_root() / ("session-" + identifier + ".lock")):
        record = next(r for r in catalog_read()["sessions"] if r["id"] == identifier)
        provider = record["provider"]
        if provider not in ("codex", "claude") or shutil.which(provider) is None:
            raise RuntimeError(provider + " backend is not installed or connected")
        agents = api(binary, "agent.list", {})["agents"]
        existing = next((a for a in agents if same_reference(a, record)), None)
        if existing:
            if provider == "codex":
                note_session_open(existing["pane_id"])
            return {"agent": existing}
        # A start whose transport result was lost is inspected, never replayed.
        pending = next((a for a in agents if record["terminal_id"] and a["terminal_id"] == record["terminal_id"] and (a.get("launch_pending") or a.get("agent") == provider)), None)
        if pending:
            if provider == "codex":
                try:
                    bind(pending["pane_id"], binary, (record.get("reference") or {}).get("value"))
                except (RuntimeError, OSError):
                    pass
            return {"agent": next((a for a in api(binary, "agent.list", {})["agents"] if a["terminal_id"] == pending["terminal_id"]), pending)}
        old_pane = record["pane_id"]
        owner = None
        if provider == "codex" and old_pane:
            candidate = None
            if registry_path(old_pane).exists():
                saved = json.loads(registry_path(old_pane).read_text())
                try:
                    candidate = read_registry(old_pane)
                except (RuntimeError, OSError):
                    live = False
                    try:
                        live = start_ticks(saved["pid"]) == saved["start_ticks"]
                    except OSError:
                        pass
                    with socket.socket(socket.AF_UNIX) as probe:
                        probe.settimeout(1)
                        listening = probe.connect_ex(saved["socket"]) == 0
                    if live or listening:
                        raise RuntimeError("existing session backend is unavailable; inspect it before restoring")
            if candidate:
                # A timed-out/paused live owner is not absence of an owner.
                loaded = backend_rpc(old_pane, "thread/loaded/list", {})["data"]
                reference = record.get("reference")
                if (reference and reference["value"] in loaded) or not loaded:
                    owner = candidate
        reference = record.get("reference")
        if provider == "codex" and reference and not provider_history_exists(reference):
            # Empty legacy sessions may not have a provider file yet. Never stop
            # a live owner merely to recreate an empty conversation.
            if owner and reference["value"] in backend_rpc(old_pane, "thread/loaded/list", {})["data"]:
                raise RuntimeError("unpersisted session is still loaded; keep its original terminal or retry after it closes")
            reference = None
            record = patch_session(identifier, {"reference": None})
        pane = None
        if old_pane:
            try:
                process = api(binary, "pane.process_info", {"pane_id": old_pane})["process_info"]
                if process["shell_pid"] == process["foreground_process_group_id"]:
                    pane = old_pane
            except RuntimeError as error:
                if "not_found" not in str(error):
                    raise
        name = "kite-" + provider + "-" + identifier[:8]
        if pane is None:
            created = api(binary, "workspace.create", {"cwd": record.get("cwd"), "label": name, "focus": False})
            pane = created["root_pane"]["pane_id"]
            record = patch_session(identifier, {"workspace_id": created["workspace"]["workspace_id"], "pane_id": pane, "terminal_id": created["root_pane"]["terminal_id"], "agent_name": name})
        else:
            record = patch_session(identifier, {"agent_name": name})
        if provider == "codex":
            if owner and owner["pane"] != pane:
                alias = dict(owner, pane=pane)
                atomic_json(registry_path(pane), alias)
                info = read_registry(pane)
            else:
                info = ensure(pane, binary)
            note_session_open(pane)
            # Warm read-only account/routing discovery before the TUI's shorter
            # bootstrap deadline. This does not start a turn or change credentials.
            try:
                backend_rpc(pane, "account/read", {"refreshToken": False})
            except RuntimeError:
                pass
            args = ["--remote", "unix://" + info["socket"], "-c", "check_for_update_on_startup=false"]
            if reference:
                args += ["resume", reference["value"]]
        else:
            args = []
            if reference:
                if reference["kind"] == "id":
                    value = reference["value"]
                elif reference["kind"] == "path":
                    value = Path(reference["value"]).stem
                else:
                    raise ValueError("unsupported Claude session reference")
                if not re.fullmatch(r"[a-zA-Z0-9_-]+", value):
                    raise ValueError("invalid Claude session ID")
                args = ["--resume", value]
        started = api(binary, "agent.start", {"pane_id": pane, "name": name, "kind": provider, "args": args})["agent"]
        patch_session(identifier, {"terminal_id": started["terminal_id"], "pane_id": pane, "workspace_id": started["workspace_id"], "agent_name": name})
        for _ in range(12):
            if provider == "codex":
                try:
                    bind(pane, binary, reference["value"] if reference else None)
                except (RuntimeError, OSError):
                    pass
            current = next((a for a in api(binary, "agent.list", {})["agents"] if a["terminal_id"] == started["terminal_id"]), None)
            if current:
                started = current
                if current.get("agent_session"):
                    patch_session(identifier, {"reference": current["agent_session"]})
                    return {"agent": current}
            time.sleep(.2)
        return {"agent": started}


def upload(identifier, name, size, digest):
    if not re.fullmatch(r"[a-f0-9]{32}", identifier) or not 0 < size <= MAX_FILE:
        raise ValueError("invalid upload")
    if not name or len(name.encode()) > 180 or name in (".", "..") or any(c in name for c in "/\\\0\r\n"):
        raise ValueError("invalid filename")
    root = private_dir(Path.home() / ".local/share/kite/uploads")
    directory = root / identifier
    directory.mkdir(mode=0o700)  # Each attempt is unique; never overwrite an existing upload.
    temporary = directory / ".part"
    try:
        checksum = hashlib.sha256()
        remaining = size
        with temporary.open("xb") as dest:
            temporary.chmod(0o600)
            while remaining:
                chunk = sys.stdin.buffer.read(min(65536, remaining))
                if not chunk:
                    raise ValueError("incomplete upload")
                checksum.update(chunk)
                dest.write(chunk)
                remaining -= len(chunk)
            dest.flush()
            os.fsync(dest.fileno())
        if checksum.hexdigest() != digest:
            raise ValueError("upload checksum mismatch")
        final = directory / name
        temporary.rename(final)
        return {"path": str(final), "sha256": digest, "size": size}
    except BaseException:
        temporary.unlink(missing_ok=True)
        directory.rmdir()
        raise


def read_uploaded_attachment(identifier, name, output):
    if not re.fullmatch(r"[a-f0-9]{32}", identifier or ""):
        raise ValueError("invalid attachment identity")
    if not name or len(name.encode()) > 180 or name in (".", "..") or any(c in name for c in "/\\\0\r\n"):
        raise ValueError("invalid attachment filename")
    root = private_dir(Path.home() / ".local/share/kite/uploads")
    descriptor = os.open(root / identifier, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        file_descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptor)
        with os.fdopen(file_descriptor, "rb") as source:
            info = os.fstat(source.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or not 0 < info.st_size <= MAX_FILE:
                raise ValueError("invalid attachment file")
            data = source.read(MAX_FILE + 1)
        if len(data) != info.st_size:
            raise ValueError("attachment changed during read")
        header = dict(name=name, size=len(data), sha256=hashlib.sha256(data).hexdigest())
        output.write(json.dumps(header, ensure_ascii=False).encode() + b"\n")
        output.write(data)
    finally:
        os.close(descriptor)


def proxy(pane):
    """Adapt JSONL to the documented Unix WebSocket transport with bounded frames."""
    info = read_registry(pane)
    limit = 2 * 1024 * 1024
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(20)
        connection.connect(info["socket"])
        key = base64.b64encode(os.urandom(16)).decode()
        connection.sendall(("GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: " + key + "\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        header = bytearray()
        while not header.endswith(b"\r\n\r\n"):
            byte = connection.recv(1)
            if not byte:
                raise EOFError("incomplete WebSocket handshake")
            header.extend(byte)
            if len(header) > 8192:
                raise RuntimeError("oversized WebSocket handshake")
        expected = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest())
        fields = {key.lower(): value.strip() for line in bytes(header).split(b"\r\n")[1:] if b":" in line for key, value in [line.split(b":", 1)]}
        if not header.startswith(b"HTTP/1.1 101 ") or fields.get(b"sec-websocket-accept") != expected:
            raise RuntimeError("WebSocket handshake rejected")

        def exact(count):
            data = bytearray()
            while len(data) < count:
                chunk = connection.recv(count - len(data))
                if not chunk:
                    raise EOFError("backend disconnected")
                data.extend(chunk)
            return bytes(data)

        def frame(opcode, data):
            mask = os.urandom(4)
            size = len(data)
            prefix = bytes([0x80 | opcode])
            if size < 126:
                prefix += bytes([0x80 | size])
            elif size < 65536:
                prefix += b"\xfe" + struct.pack("!H", size)
            else:
                prefix += b"\xff" + struct.pack("!Q", size)
            connection.sendall(prefix + mask + bytes(value ^ mask[i % 4] for i, value in enumerate(data)))

        with selectors.DefaultSelector() as selected:
            selected.register(sys.stdin.fileno(), selectors.EVENT_READ, "stdin")
            selected.register(connection, selectors.EVENT_READ, "socket")
            stdin = bytearray()
            fragments = bytearray()
            continuing = False
            while True:
                for entry, _ in selected.select():
                    if entry.data == "stdin":
                        chunk = os.read(sys.stdin.fileno(), 65536)
                        if not chunk:
                            frame(8, b"")
                            return
                        stdin.extend(chunk)
                        while b"\n" in stdin:
                            line, _, remainder = stdin.partition(b"\n")
                            if len(line) > limit:
                                raise ValueError("oversized RPC")
                            frame(1, line)
                            stdin = bytearray(remainder)
                        if len(stdin) > limit:
                            raise ValueError("oversized RPC")
                    else:
                        a, b = exact(2)
                        opcode, final = a & 15, bool(a & 128)
                        if a & 0x70 or b & 128:
                            raise RuntimeError("invalid server WebSocket frame")
                        size = b & 127
                        if size == 126:
                            size = struct.unpack("!H", exact(2))[0]
                        elif size == 127:
                            size = struct.unpack("!Q", exact(8))[0]
                        if size > limit or len(fragments) + size > limit:
                            raise RuntimeError("oversized backend frame")
                        if opcode >= 8 and (not final or size > 125):
                            raise RuntimeError("invalid control frame")
                        data = exact(size)
                        if opcode == 8:
                            frame(8, data[:125])
                            return
                        if opcode == 9:
                            frame(10, data)
                            continue
                        if opcode == 10:
                            continue
                        if opcode not in (0, 1) or (opcode == 0) != continuing:
                            raise RuntimeError("unexpected backend frame")
                        fragments.extend(data)
                        continuing = not final
                        if final:
                            # JSONL messages must contain no literal line break.
                            if b"\n" in fragments:
                                raise RuntimeError("invalid JSONL frame")
                            sys.stdout.buffer.write(fragments + b"\n")
                            sys.stdout.buffer.flush()
                            fragments.clear()


def control_line(stream, limit):
    """Read one source record with image strings removed and bounded memory."""
    pattern = re.compile(rb'"image_url"\s*:\s*"')
    result = bytearray()
    carry = b""
    skipping = escaped = complete = False
    def emit(data):
        result.extend(data[:max(0, limit + 1 - len(result))])
    while True:
        chunk = stream.readline(65536)
        if not chunk:
            break
        complete = chunk.endswith(b"\n")
        data = carry + chunk
        carry = b""
        while data:
            if skipping:
                position = data.find(b'"')
                while position >= 0:
                    back = position - 1
                    while back >= 0 and data[back] == 92:
                        back -= 1
                    odd = (position - back - 1) % 2 == 1
                    if back < 0:
                        odd ^= escaped
                    if not odd:
                        break
                    position = data.find(b'"', position + 1)
                if position < 0:
                    back = len(data) - 1
                    while back >= 0 and data[back] == 92:
                        back -= 1
                    escaped = (len(data) - back - 1) % 2 == 1
                    data = b""
                    continue
                data = data[position+1:]
                skipping = escaped = False
            else:
                match = pattern.search(data)
                if match:
                    emit(data[:match.start()] + b'"image_url":""')
                    data = data[match.end():]
                    skipping = True
                else:
                    if len(data) > 64:
                        emit(data[:-64])
                    carry = data[-64:]
                    break
        if complete:
            break
    emit(carry)
    return (bytes(result) if len(result) <= limit else None), complete


def question_records(file):
    """Index raw control facts, not pending/resolved business state, across the full log."""
    source = Path(file)
    directory = private_dir(state_dir() / "history")
    cache = directory / (hashlib.sha256(str(source.resolve()).encode()).hexdigest() + ".json")
    limit = 2 * 1024 * 1024
    with file_lock(cache.with_suffix(".lock")), source.open("rb") as stream:
        info = os.fstat(stream.fileno())
        index = {"version": 3, "device": info.st_dev, "inode": info.st_ino, "offset": 0, "rows": []}
        try:
            saved = json.loads(cache.read_text())
            if saved["version"] == 3 and saved["device"] == info.st_dev and saved["inode"] == info.st_ino and 0 <= saved["offset"] <= info.st_size:
                index = saved
        except (OSError, ValueError, KeyError, TypeError):
            pass
        calls = {row["payload"]["call_id"] for row in index["rows"] if row["payload"].get("type") == "function_call"}
        stream.seek(index["offset"])
        changed = False
        while True:
            start = stream.tell()
            line, complete = control_line(stream, limit)
            if not complete:
                # Retry a partially written source record on the next snapshot.
                stream.seek(start)
                break
            index["offset"] = stream.tell()
            changed = True
            if line is None or not any(token in line for token in (b'"function_call"', b'"function_call_output"', b'"user"', b'"task_complete"', b'"task_aborted"')):
                continue
            try:
                record = json.loads(line)
                if not isinstance(record, dict) or record.get("type") not in ("response_item", "event_msg") or not isinstance(record.get("payload"), dict):
                    continue
                payload = record["payload"]
                kind = payload.get("type")
                keep = False
                if record.get("type")=="event_msg" and kind in ("task_complete","task_aborted"):
                    record={"type":"event_msg","payload":{"type":kind,"turn_id":str(payload.get("turn_id") or start)}}
                    keep=True
                elif kind == "function_call" and payload.get("name") == "request_user_input_async" and payload.get("call_id"):
                    calls.add(payload["call_id"])
                    keep = True
                elif kind == "function_call_output" and payload.get("call_id") in calls:
                    keep = True
                elif kind == "message" and payload.get("role") == "user":
                    # Preserve the user-input boundary without copying ordinary
                    # message bodies or image data into the control index.
                    content = payload.get("content", [])
                    text = [part for part in content if isinstance(part, dict) and isinstance(part.get("text"), str)] if isinstance(content, list) else []
                    if not any("<send_user_message_question_reply>" in part["text"] for part in text):
                        marker = next((tag for tag in ("<environment_context>", "<turn_aborted>") if text and text[0]["text"].startswith(tag)), None)
                        text = [{"type":"input_text","text":marker}] if marker else []
                    record = {"type":"response_item","payload":{"type":"message","role":"user","content":text}}
                    keep = True
                if keep:
                    index["rows"].append(record)
            except (ValueError, KeyError, TypeError):
                continue
        # Ordinary user boundaries carry no body. Repeated adjacent boundaries
        # are equivalent for question resolution; don't grow this control index
        # with the entire ordinary conversation.
        compact = []
        last_terminal=next((i for i in range(len(index["rows"])-1,-1,-1) if index["rows"][i].get("type")=="event_msg"),None)
        for i,row in enumerate(index["rows"]):
            if row.get("type")=="event_msg" and i!=last_terminal:continue
            ordinary = row.get("payload", {}).get("role") == "user" and row["payload"].get("content") == []
            if ordinary and compact and row == compact[-1]:
                continue
            compact.append(row)
        if len(compact) != len(index["rows"]):
            index["rows"] = compact
            changed = True
        records = b"".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")).encode() + b"\n" for row in index["rows"])
        if len(records) > 1024 * 1024:
            raise ValueError("question history index exceeds transport capacity")
        if changed or not cache.exists():
            atomic_json(cache, index)
        return records


def reverse_record_ranges(stream, end):
    """Yield source line boundaries backwards using constant-size scan buffers."""
    position = line_end = end
    while position:
        start = max(0, position - 65536)
        stream.seek(start)
        block = stream.read(position - start)
        index = len(block)
        while True:
            index = block.rfind(b"\n", 0, index)
            if index < 0:
                break
            line_start = start + index + 1
            if line_start < line_end:
                yield line_start, line_end
            line_end = line_start
        position = start
    if line_end:
        yield 0, line_end


def history_cursor(info, position):
    return f"{info.st_dev}:{info.st_ino}:{position}:{info.st_size}"


def cursor_position(value, info):
    if not re.fullmatch(r"\d+:\d+:\d+:\d+", value or "") or len(value) > 128:
        raise ValueError("invalid history cursor")
    device, inode, position, size = map(int, value.split(":"))
    if device != info.st_dev or inode != info.st_ino or not 0 <= position <= size <= info.st_size:
        raise ValueError("history source changed; reopen the latest page")
    return position


def image_store():
    return private_dir(state_dir() / "generated-images")


def store_generated_image(encoded):
    if not isinstance(encoded, str) or len(encoded) > (MAX_FILE + 2) // 3 * 4 + 128:
        raise ValueError("generated image exceeds 25 MiB")
    if encoded.startswith("data:image/"):
        encoded = encoded.split(",", 1)[1]
    return store_generated_bytes(base64.b64decode(encoded, validate=True))


def store_generated_bytes(data):
    if not 0 < len(data) <= MAX_FILE:
        raise ValueError("invalid generated image size")
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        mime, extension = "image/png", "png"
    elif data.startswith(b"\xff\xd8\xff"):
        mime, extension = "image/jpeg", "jpg"
    elif data.startswith(b"RIFF") and data[8:12] == b"WEBP":
        mime, extension = "image/webp", "webp"
    else:
        raise ValueError("unsupported generated image format")
    identifier = hashlib.sha256(data).hexdigest()
    path = image_store() / identifier
    if not path.is_file() or path.stat().st_size != len(data) or hashlib.sha256(path.read_bytes()).hexdigest() != identifier:
        with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
            output.write(data)
            temporary = Path(output.name)
        temporary.chmod(0o600)
        temporary.replace(path)
    return dict(id=identifier, name="generated-" + identifier[:12] + "." + extension,
                mime=mime, size=len(data), path="kite-image:" + identifier)


def read_generated_image(identifier, output):
    if not re.fullmatch(r"[a-f0-9]{64}", identifier or ""):
        raise ValueError("invalid generated image identity")
    path = image_store() / identifier
    with path.open("rb") as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= MAX_FILE:
            raise ValueError("invalid generated image file")
        data = source.read(MAX_FILE + 1)
    if hashlib.sha256(data).hexdigest() != identifier:
        raise ValueError("generated image changed")
    output.write(data)


def display_line(stream, limit):
    # Native image results exceed the transcript budget. Decode them on the host
    # and send only an immutable attachment identity. Other records retain the
    # streaming image-input stripping and existing text limit.
    start = stream.tell()
    header = stream.read(1024)
    stream.seek(start)
    if not any(token in header for token in (b'"image_generation_call"', b'"image_generation_end"', b'"ImageGeneration"', b'"image_gen.generation"')):
        return control_line(stream, limit)
    line = stream.readline((MAX_FILE + 2) // 3 * 4 + limit + 1)
    complete = line.endswith(b"\n")
    if not complete:
        # Advance to the source boundary without growing memory.
        while line and not line.endswith(b"\n"):
            line = stream.readline(65536)
        if not line:
            return None, False
        # A too-large completed result must not make unrelated chat unreadable.
        identity = re.search(rb'"(?:id|call_id)"\s*:\s*"([^"\\]{1,256})"', header)
        payload = dict(type="image_generation_call", id=identity.group(1).decode() if identity else f"image-offset-{start}",
                       status="completed", _kite_image_error="generated image exceeds 25 MiB")
        return json.dumps(dict(type="response_item", payload=payload)).encode() + b"\n", True
    try:
        record = json.loads(line)
    except (ValueError, TypeError):
        return b"", True
    if not isinstance(record, dict) or not isinstance(record.get("payload"), dict):
        return b"", True
    payload = record["payload"]
    if record.get("type") == "event_msg" and payload.get("type") in ("item_completed", "item_started"):
        item = payload.get("item", {})
        if not isinstance(item, dict):
            return b"", True
        if item.get("type") == "ImageGeneration" or (item.get("type") == "Extension" and item.get("kind") == "image_gen.generation"):
            payload = dict(item, type="image_generation_call")
            payload["saved_path"] = payload.pop("savedPath", payload.get("saved_path"))
            record.update(type="response_item", payload=payload)
    if record.get("type") == "event_msg" and payload.get("type") == "image_generation_end":
        record["type"] = "response_item"
        payload["type"] = "image_generation_call"
        payload["id"] = payload.get("call_id")
    elif record.get("type") != "response_item" or payload.get("type") != "image_generation_call":
        stream.seek(start)
        return control_line(stream, limit)
    encoded = payload.pop("result", "")
    payload.pop("encrypted_content", None)
    payload["_kite_images"] = []
    try:
        if encoded:
            payload["_kite_images"] = [store_generated_image(encoded)]
        elif payload.get("saved_path") and payload.get("status") == "completed":
            path = Path(payload["saved_path"])
            if not path.is_absolute():
                raise ValueError("invalid generated image path")
            with path.open("rb") as source:
                if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
                    raise ValueError("invalid generated image file")
                payload["_kite_images"] = [store_generated_bytes(source.read(MAX_FILE + 1))]
    except (ValueError, OSError) as error:
        payload["_kite_image_error"] = str(error)
    return json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode() + b"\n", True


def display_record(line):
    try:
        record = json.loads(line)
        payload = record.get("payload")
        return record.get("type") == "response_item" and isinstance(payload, dict) and payload.get("type") in {
            "message", "function_call", "custom_tool_call", "computer_call",
            "function_call_output", "custom_tool_call_output", "computer_call_output", "image_generation_call",
        }
    except (ValueError, AttributeError, TypeError):
        return False


def history_projection(file, output, before=None, after=None):
    """Project complete source records with bidirectional, append-safe cursors."""
    if before is not None and after is not None:
        raise ValueError("choose one history direction")
    controls = question_records(file)
    transport_budget = 4 * 1024 * 1024 - len(controls) - 4096
    page_budget = min(1024 * 1024, transport_budget)
    rows = []
    total = 0
    with Path(file).open("rb") as stream:
        info = os.fstat(stream.fileno())
        boundary = cursor_position(before or after, info) if before is not None or after is not None else info.st_size

        def forward_ranges():
            position = boundary
            while position < info.st_size:
                stream.seek(position)
                line, complete = display_line(stream, transport_budget)
                end = stream.tell()
                if not complete or end > info.st_size:
                    break
                yield position, end, line
                position = end

        def backward_ranges():
            for start, end in reverse_record_ranges(stream, boundary):
                stream.seek(start)
                line, complete = display_line(stream, transport_budget)
                if complete and stream.tell() == end:
                    yield start, end, line

        for start, end, line in (forward_ranges() if after is not None else backward_ranges()):
            if line is None:
                stream.seek(start)
                header = stream.read(min(1024,end-start))
                if b'response_item' in header:
                    raise ValueError("conversation record exceeds the history transport limit")
                continue
            if not display_record(line):
                continue
            # Transport-only record position. Raw provider files stay unchanged.
            line = line.rstrip()[:-1] + b',"_kite_offset":' + str(start).encode() + b'}\n'
            if rows and (len(rows) >= 2048 or total + len(line) > page_budget):
                break
            rows.append((start, end, line))
            total += len(line)
            if total >= page_budget:
                break
        if after is None:
            rows.reverse()
        start = rows[0][0] if rows else (0 if after is None else boundary)
        end = rows[-1][1] if rows else (info.st_size if after is not None else boundary)
        page = {
            "type": "kite_history_page",
            "start": history_cursor(info, start),
            "end": history_cursor(info, end),
            "has_older": bool(rows) and start > 0,
            "has_newer": bool(rows) and end < info.st_size and (before is not None or after is not None),
        }
        body = json.dumps(page, separators=(",", ":")).encode() + b"\n" + b"".join(row[2] for row in rows)
        output.write(b"kite-codex-history-v2\n" + str(len(controls)).encode() + b"\n" + controls + body)


def notification_facts(binary, include_archived=False):
    """Return bounded host facts; transition and pending-question policy stays in Rust."""
    agents = api(binary, "agent.list", {})["agents"]
    result = []
    for record in catalog_read()["sessions"]:
        if record.get("archived") and not include_archived:
            continue
        agent = next((a for a in agents if same_reference(a, record) or
                      (not record.get("reference") and a["terminal_id"] == record.get("terminal_id"))), None)
        if agent is None or agent.get("launch_pending"):
            continue
        reference = record.get("reference") or {}
        source = agent["terminal_id"] + ":" + hashlib.sha256(json.dumps(reference, sort_keys=True).encode()).hexdigest()
        controls, known = "", True
        if reference.get("agent") == "codex":
            root = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")) / "sessions"
            try:
                if reference.get("kind") == "id" and re.fullmatch(r"[a-f0-9-]{36}", reference.get("value", "")):
                    sources = list(root.rglob("rollout-*-" + reference["value"] + ".jsonl"))
                    if len(sources) > 1:
                        raise ValueError("ambiguous source")
                    file = sources[0] if sources else None
                elif reference.get("kind") == "path":
                    file = Path(reference["value"]).resolve(strict=True)
                else:
                    file = None
                if file is not None:
                    if not file.resolve().is_relative_to(root.resolve()) or file.suffix != ".jsonl":
                        raise ValueError("invalid history source")
                    controls = question_records(file).decode("utf-8")
            except (OSError, ValueError, KeyError, RuntimeError):
                known = False
        result.append(dict(session_id=record["id"], title=record["title"][:160], source=source,
                           status=agent["agent_status"], revision=agent.get("state_change_seq", 0),
                           thread=reference.get("value", ""), controls=controls, controls_known=known, lifecycle=reference.get("agent")=="codex"))
    output = json.dumps(dict(facts=result), ensure_ascii=False)
    if len(output.encode()) > 4 * 1024 * 1024:
        raise ValueError("notification facts exceed transport capacity")
    return output


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["ensure", "proxy", "upgrade", "upload", "bind", "identity", "tui", "history", "catalog-read", "catalog-write", "session-open", "notification-facts", "session-facts", "lifecycle-facts", "lifecycle-reap", "image-read", "attachment-read"])
    parser.add_argument("--pane")
    parser.add_argument("--herdr")
    parser.add_argument("--thread")
    parser.add_argument("--file")
    parser.add_argument("--compressed", action="store_true")
    parser.add_argument("--before")
    parser.add_argument("--after")
    parser.add_argument("--id")
    parser.add_argument("--name")
    parser.add_argument("--size", type=int)
    parser.add_argument("--sha256")
    args = parser.parse_args()
    if args.action == "proxy":
        proxy(args.pane)
    elif args.action == "upload":
        print(json.dumps(upload(args.id, args.name, args.size, args.sha256)), flush=True)
    elif args.action == "ensure":
        print(json.dumps(ensure(args.pane, args.herdr)))
    elif args.action == "bind":
        print(json.dumps(bind(args.pane, args.herdr)))
    elif args.action == "identity":
        print(json.dumps(read_registry(args.pane)))
    elif args.action == "history":
        if args.compressed:
            with gzip.GzipFile(fileobj=sys.stdout.buffer,mode="wb",compresslevel=1,mtime=0) as output:
                history_projection(args.file, output, args.before, args.after)
        else:
            history_projection(args.file, sys.stdout.buffer, args.before, args.after)
    elif args.action == "attachment-read":
        read_uploaded_attachment(args.id, args.name, sys.stdout.buffer)
    elif args.action == "image-read":
        read_generated_image(args.id, sys.stdout.buffer)
    elif args.action == "session-facts":
        print(notification_facts(args.herdr, include_archived=True))
    elif args.action == "lifecycle-facts":
        print(json.dumps(lifecycle_facts(args.herdr)))
    elif args.action == "lifecycle-reap":
        payload = sys.stdin.buffer.read(65537)
        if len(payload) > 65536:
            raise ValueError("lifecycle request too large")
        print(json.dumps(lifecycle_reap(json.loads(payload), args.herdr)))
    elif args.action == "notification-facts":
        print(notification_facts(args.herdr))
    elif args.action == "catalog-read":
        print(json.dumps(catalog_read()))
    elif args.action == "catalog-write":
        payload = sys.stdin.buffer.read(1048577)
        if len(payload) > 1048576:
            raise ValueError("session catalog too large")
        print(json.dumps(catalog_write(json.loads(payload))))
    elif args.action == "session-open":
        print(json.dumps(session_open(args.id, args.herdr)))
    elif args.action == "tui":
        info = ensure(args.pane, args.herdr)
        note_session_open(args.pane)
        if not re.fullmatch(r"[a-f0-9-]{36}", args.thread or ""):
            raise ValueError("invalid thread ID")
        os.execvp("codex", ["codex", "--remote", "unix://" + info["socket"], "-c", "check_for_update_on_startup=false", "resume", args.thread])
    else:
        print(json.dumps(upgrade(args.pane, args.herdr)))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(type(error).__name__ + ": " + str(error), file=sys.stderr)
        sys.exit(1)
