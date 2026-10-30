"""One bounded clipboard operation in the same user's captured desktop session."""
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import sys
import tempfile
import time

LIMIT = 256 * 1024


def environment(port):
    if sys.platform == "darwin":
        return os.environ.copy(), "macos"
    if not sys.platform.startswith("linux"):
        raise RuntimeError("clipboard platform unsupported")
    result = subprocess.run(["ss", "-H", "-lntp", f"sport = :{port}"], capture_output=True, text=True, timeout=2, check=True)
    pids = set(re.findall(r"pid=(\d+)", result.stdout))
    if len(pids) != 1:
        raise RuntimeError("captured desktop process unavailable")
    proc = Path("/proc") / next(iter(pids))
    if proc.stat().st_uid != os.getuid() or (proc / "comm").read_text().strip() != "sunshine":
        raise RuntimeError("captured desktop process identity mismatch")
    allowed = {"DISPLAY", "XAUTHORITY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"}
    values = {}
    for item in (proc / "environ").read_bytes().split(b"\0"):
        if b"=" in item:
            name, value = item.split(b"=", 1)
            if name.decode(errors="replace") in allowed:
                values[name.decode()] = value.decode()
    if not values.get("DISPLAY") and not values.get("WAYLAND_DISPLAY"):
        raise RuntimeError("captured desktop environment unavailable")
    env = os.environ.copy()
    for name in allowed:
        env.pop(name, None)
    env.update(values)
    return env, "linux"


def executable(name):
    local = Path.home() / ".local/lib/kite/bin" / name
    path = str(local) if local.is_file() and os.access(local, os.X_OK) else shutil.which(name)
    if not path:
        raise RuntimeError("clipboard tool unavailable: " + name)
    return path


def transfer(mode, data, env, platform):
    if mode not in ("read", "write") or len(data) > LIMIT:
        raise ValueError("invalid clipboard operation or size")
    if platform == "macos":
        args = ["/usr/bin/pbpaste" if mode == "read" else "/usr/bin/pbcopy"]
    elif env.get("WAYLAND_DISPLAY"):
        args = [executable("wl-paste"), "--no-newline", "--type", "text"] if mode == "read" else [executable("wl-copy"), "--type", "text/plain;charset=utf-8"]
    else:
        args = [executable("xclip"), "-selection", "clipboard", "-out", "-target", "UTF8_STRING"] if mode == "read" else [executable("xclip"), "-selection", "clipboard", "-in", "-target", "UTF8_STRING"]
    with tempfile.TemporaryFile() as errors:
        proc = subprocess.Popen(args, stdin=subprocess.PIPE if mode == "write" else subprocess.DEVNULL, stdout=subprocess.PIPE if mode == "read" else subprocess.DEVNULL, stderr=errors, env=env, start_new_session=True)
        try:
            if mode == "write":
                proc.communicate(data, timeout=3)
                result = b""
            else:
                result = bytearray()
                deadline = time.monotonic() + 3
                while True:
                    if not select.select([proc.stdout], [], [], max(0, deadline-time.monotonic()))[0]:
                        raise TimeoutError("clipboard read timed out")
                    chunk = os.read(proc.stdout.fileno(), min(65536, LIMIT + 1 - len(result)))
                    if not chunk:
                        break
                    result.extend(chunk)
                    if len(result) > LIMIT:
                        raise ValueError("clipboard text exceeds limit")
                proc.wait(timeout=max(.1, deadline-time.monotonic()))
            if proc.returncode:
                errors.seek(0)
                diagnostic = errors.read(4096).strip()
                if mode == "read" and platform == "linux" and not env.get("WAYLAND_DISPLAY") and diagnostic == b"Error: target UTF8_STRING not available":
                    return ""
                raise RuntimeError("clipboard operation failed")
            return bytes(result).decode("utf-8")
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=2)
            if proc.stdout:
                proc.stdout.close()


def main():
    mode, port = sys.argv[1:]
    port = int(port)
    if not 1 <= port <= 65535:
        raise ValueError("invalid desktop port")
    data = sys.stdin.buffer.read(LIMIT + 1) if mode == "write" else b""
    env, platform = environment(port)
    text = transfer(mode, data, env, platform)
    print(json.dumps({"platform": platform, "text": text}, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(type(error).__name__ + ": " + str(error), file=sys.stderr)
        raise SystemExit(1)
