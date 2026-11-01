"""Native DSH SDK transport. No model turn during discovery."""
import json
import os
import queue
import signal
import subprocess
import threading

LIMIT = 2 * 1024 * 1024


class Native:
    def __init__(self, provider, command, cwd, model, event=lambda value: None):
        self.provider, self.model, self.event = provider, model, event
        self.pending, self.counter, self.lock = {}, 0, threading.Lock()
        self.idle = threading.Event()
        self.running = False
        self.received_reply = False
        self.session = None
        self.failed = False
        self.closing = False
        self.child = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                      start_new_session=True)
        threading.Thread(target=self.read, daemon=True).start()
        try:
            value = self.call("initialize", {"cwd": cwd,
                "provider": os.environ.get("KITE_DSH_PROVIDER", "deepseek-official"), "model": model}, 25)
            if value.get("serverInfo", {}).get("name") != "deepseek-harness-sdk-runtime":
                raise RuntimeError("DSH SDK identity mismatch")
        except Exception:
            self.close()
            raise

    def write(self, value):
        data = (json.dumps(value, ensure_ascii=False) + "\n").encode()
        if len(data) > LIMIT:
            raise ValueError("native request exceeds limit")
        with self.lock:
            self.child.stdin.write(data)
            self.child.stdin.flush()

    def call(self, method, params, timeout=3600):
        if self.closing or self.child.poll() is not None:
            raise RuntimeError("native runtime is closed")
        with self.lock:
            self.counter += 1
            identifier = str(self.counter)
            response = self.pending[identifier] = queue.Queue(1)
        try:
            self.write({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
            try:
                value = response.get(timeout=timeout)
            except queue.Empty:
                raise RuntimeError("native request timed out; delivery may have occurred") from None
            if "error" in value:
                raise RuntimeError(self.provider + " native request failed")
            return value.get("result", {})
        finally:
            self.pending.pop(identifier, None)

    def read(self):
        try:
            while line := self.child.stdout.readline(LIMIT + 1):
                if len(line) > LIMIT or not line.endswith(b"\n"):
                    break
                value = json.loads(line)
                if "id" in value and "method" not in value:
                    response = self.pending.get(str(value["id"]))
                    if response is not None:
                        response.put_nowait(value)
                elif value.get("method") == "session.event":
                    params = value.get("params", {})
                    if params.get("sessionId") == self.session:
                        event = params.get("event", {})
                        if event.get("type") == "assistant/message":
                            blocks = event.get("data", {}).get("message", {}).get("content", [])
                            self.received_reply |= any(b.get("type") == "text" and b.get("text", "").strip() for b in blocks)
                        if event.get("type") == "turn/end":
                            reason = event.get("data", {}).get("reason", {})
                            self.failed |= reason.get("kind") not in (None, "completed")
                        self.event(event)
                elif value.get("method") == "session.status":
                    params = value.get("params", {})
                    if params.get("sessionId") == self.session:
                        if params.get("status") == "running":
                            self.running = True
                        elif params.get("status") == "idle" and self.running:
                            self.idle.set()
        except (ValueError, OSError, KeyError, TypeError, AttributeError, queue.Full):
            pass
        finally:
            self.failed = True
            self.idle.set()
            for response in list(self.pending.values()):
                try:
                    response.put_nowait({"error": {}})
                except queue.Full:
                    pass
            if not self.closing:
                self.close()

    def run(self, thread, prompt):
        self.session, self.failed = thread, False
        self.running = False
        self.received_reply = False
        self.idle.clear()
        receipt = self.call("session/prompt", {"sessionId": thread,
            "contentBlocks": [{"type": "text", "text": prompt}]}, 25)
        if not isinstance(receipt.get("messageId"), str):
            raise RuntimeError("DSH did not acknowledge prompt")
        if not self.idle.wait(3600) or self.failed:
            raise RuntimeError("DSH turn failed or disconnected; inspect history before retrying")
        if not self.received_reply:
            raise RuntimeError("DSH returned no public assistant reply")
        return None

    def close(self):
        self.closing = True
        if self.child.poll() is None:
            try:
                os.killpg(self.child.pid, signal.SIGTERM)
                self.child.wait(timeout=3)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                if self.child.poll() is None:
                    os.killpg(self.child.pid, signal.SIGKILL)
                    self.child.wait(timeout=3)
        self.child.stdin.close()
        self.child.stdout.close()
