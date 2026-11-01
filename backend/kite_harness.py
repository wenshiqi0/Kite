"""Opt-in provider discovery and private, durable Kite harness owners."""
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import socket
import subprocess
import sys
import time
import uuid
import tempfile
import urllib.request
from urllib.parse import urlsplit

from kite_harness_native import Native

IDS = ("codex", "claude", "dsh")


def start_ticks(pid):
    try:
        return (Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split())[19]
    except (OSError, IndexError):
        return None


def root():
    override = os.environ.get("KITE_HARNESS_STATE_DIR")
    path = Path(override) if override else Path.home() / ".local/state/kite/harnesses"
    if not path.is_absolute():
        raise ValueError("harness state directory must be absolute")
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    return path


def directory(thread):
    if str(uuid.UUID(thread)) != thread:
        raise ValueError("invalid harness reference")
    return root() / thread


def atomic(path, value):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete=False) as stream:
        pending = Path(stream.name)
        json.dump(value, stream, ensure_ascii=False)
        stream.flush()
        os.fsync(stream.fileno())
    pending.replace(path)


def configuration(provider):
    if provider != "dsh" or os.environ.get("KITE_ENABLE_" + provider.upper()) != "1":
        raise RuntimeError("harness is not enabled")
    model = os.environ.get("KITE_" + provider.upper() + "_MODEL", "deepseek-v4-flash")
    command = os.environ.get("KITE_" + provider.upper() + "_COMMAND")
    if command:
        command = json.loads(command)
    else:
        command = ["dsh", "--profile", "sdk"]
    if (not isinstance(command, list) or not command or len(command) > 24
            or any(not isinstance(s, str) or not s or "\0" in s for s in command)):
        raise ValueError("harness command must be a JSON argv array")
    executable = shutil.which(command[0])
    if executable is None:
        raise RuntimeError("harness executable unavailable")
    return [executable, *command[1:]], model


def probe(provider):
    command, model = configuration(provider)
    key = os.environ.get("MIMO_API_KEY" if model.startswith("mimo-") else "DEEPSEEK_API_KEY")
    if not key:
        raise RuntimeError("harness model credential unavailable")
    endpoint = os.environ.get("KITE_DSH_HEALTH_URL")
    if not endpoint:
        if os.environ.get("KITE_DSH_PROVIDER", "deepseek-official") != "deepseek-official":
            raise RuntimeError("custom DSH routes require a matching health URL")
        endpoint = "https://api.deepseek.com/v1/models"
    parsed = urlsplit(endpoint)
    if parsed.username or parsed.password or parsed.scheme not in ("https", "http") or (
            parsed.scheme == "http" and parsed.hostname not in ("localhost", "127.0.0.1", "::1")):
        raise ValueError("invalid harness health URL")
    request = urllib.request.Request(endpoint, headers={"Authorization": "Bearer " + key})
    with urllib.request.urlopen(request, timeout=5) as response:
        value = json.loads(response.read(1024 * 1024 + 1))
        if not any(item.get("id") == model for item in value.get("data", [])):
            raise RuntimeError("harness model unavailable")
    native = Native(provider, command, str(Path.home()), model)
    native.close()


def providers():
    # A cache belongs to this exact configuration, never another Node generation.
    names = ["PATH", "DEEPSEEK_API_KEY", "MIMO_API_KEY", "DSH_HOME"]
    values = {key: value for key, value in os.environ.items() if key.startswith("KITE_") or key in names}
    digest = hashlib.sha256(json.dumps(values, sort_keys=True).encode()).hexdigest()
    cache = root() / ("probe-" + digest + ".json")
    if cache.exists() and time.time() - cache.stat().st_mtime < 20:
        return json.loads(cache.read_text())
    result = []
    for provider in IDS:
        try:
            if provider == "dsh":
                probe(provider)
            elif shutil.which(provider) is None:
                continue
            result.append(dict(id=provider, managed_chat=provider != "claude",
                               models=provider == "codex", attachments=provider == "codex",
                               skills=provider == "codex", usage=provider == "codex",
                               pause=provider in ("codex", "claude"), terminal=provider in ("codex", "claude")))
        except (OSError, ValueError, RuntimeError):
            continue
    atomic(cache, result)
    return result


def agent(info, state):
    thread = info["thread"]
    terminal = "kite-harness-" + thread
    return dict(terminal_id=terminal, pane_id=terminal, tab_id=terminal,
                workspace_id=terminal, name=info.get("title"), title=None, agent=info["provider"],
                cwd=info["cwd"], agent_status=state.get("status", "idle"),
                revision=state.get("revision", 1), state_change_seq=state.get("revision", 1),
                launch_pending=False, interactive_ready=True,
                agent_session=dict(source="kite:harness", agent=info["provider"], kind="id", value=thread))


def agents(records):
    result = []
    for record in records:
        reference = record.get("reference") or {}
        if reference.get("source") != "kite:harness":
            continue
        try:
            path = directory(reference["value"])
            info = json.loads((path / "session.json").read_text())
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(.2)
                connection.connect(str(path / "owner.sock"))
            result.append(agent(info, json.loads((path / "state.json").read_text())))
        except (OSError, ValueError, KeyError):
            continue
    return result


def open_session(record):
    provider = record["provider"]
    command, model = configuration(provider)
    reference = record.get("reference")
    thread = reference["value"] if reference else str(uuid.uuid4())
    path = directory(thread)
    path.mkdir(mode=0o700, exist_ok=True)
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(.5)
        if connection.connect_ex(str(path / "owner.sock")) == 0:
            info = json.loads((path / "session.json").read_text())
            if info["provider"] != provider or info["id"] != record["id"]:
                raise ValueError("harness identity mismatch")
            return agent(info, json.loads((path / "state.json").read_text()))
    if (path / "state.json").exists():
        previous = json.loads((path / "state.json").read_text())
        pid = previous.get("pid")
        if isinstance(pid, int) and previous.get("start_ticks") == start_ticks(pid) and start_ticks(pid) is not None:
            raise RuntimeError("existing harness owner is unavailable; inspect it before restoring")
        if previous.get("status") == "working":
            raise RuntimeError("harness owner was interrupted; inspect its history before restoring")
    info = dict(id=record["id"], thread=thread, provider=provider,
                cwd=record.get("cwd") or str(Path.home()), title=record.get("title", ""),
                command=command, model=model)
    atomic(path / "session.json", info)
    child = subprocess.Popen([sys.executable, str(Path(__file__).with_name("kite_harness_runtime.py")), thread],
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, start_new_session=True)
    import threading
    threading.Thread(target=child.wait, daemon=True).start()
    for _ in range(300):
        if child.poll() is not None:
            raise RuntimeError("harness owner initialization failed")
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(.2)
            if connection.connect_ex(str(path / "owner.sock")) == 0:
                return agent(info, json.loads((path / "state.json").read_text()))
        time.sleep(.1)
    raise RuntimeError("harness owner initialization timed out")


def proxy(pane):
    if not pane.startswith("kite-harness-"):
        raise ValueError("invalid harness pane")
    path = directory(pane.removeprefix("kite-harness-"))
    info = json.loads((path / "session.json").read_text())
    configuration(info["provider"])
    with socket.socket(socket.AF_UNIX) as connection:
        connection.connect(str(path / "owner.sock"))
        sources = [sys.stdin.buffer, connection]
        while sources:
            ready, _, _ = select.select(sources, [], [], 30)
            for source in ready:
                data = os.read(source.fileno(), 65536)
                if not data:
                    return
                if source is connection:
                    sys.stdout.buffer.write(data)
                    sys.stdout.buffer.flush()
                else:
                    connection.sendall(data)
