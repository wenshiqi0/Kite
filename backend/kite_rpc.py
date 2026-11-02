"""Bounded JSON-RPC subprocess transport, including server permission requests."""
import json
import os
import queue
import signal
import subprocess
import threading

LIMIT = 40 * 1024 * 1024


class RpcError(RuntimeError):
    def __init__(self, code, detail=None):
        super().__init__("native request rejected")
        self.code = code
        # Diagnostic only. The owner never forwards native errors to clients.
        self.detail = detail


class TurnCancelled(RuntimeError):
    pass


class RpcProcess:
    def __init__(self, command, cwd, notification=lambda value: None, request=lambda value: None):
        self.pending, self.counter, self.lock = {}, 0, threading.Lock()
        self.notification, self.request = notification, request
        self.closing = False
        self.child = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                      start_new_session=True)
        threading.Thread(target=self.read, daemon=True).start()

    def write(self, value):
        data = (json.dumps({"jsonrpc": "2.0", **value}, ensure_ascii=False) + "\n").encode()
        if len(data) > LIMIT:
            raise ValueError("native request exceeds limit")
        with self.lock:
            self.child.stdin.write(data)
            self.child.stdin.flush()

    def call(self, method, params, timeout=40, on_id=lambda identifier: None):
        if self.closing or self.child.poll() is not None:
            raise RuntimeError("native runtime is closed")
        with self.lock:
            self.counter += 1
            identifier = str(self.counter)
            response = self.pending[identifier] = queue.Queue(1)
        try:
            self.write({"id": identifier, "method": method, "params": params})
            on_id(identifier)
            try:
                value = response.get(timeout=timeout)
            except queue.Empty:
                raise RuntimeError("native request timed out; delivery may have occurred") from None
            if "error" in value:
                raise RpcError(value["error"].get("code"), value["error"].get("message"))
            return value.get("result", {})
        finally:
            self.pending.pop(identifier, None)

    def read(self):
        try:
            while line := self.child.stdout.readline(LIMIT + 1):
                if len(line) > LIMIT or not line.endswith(b"\n"):
                    break
                value = json.loads(line)
                if "method" in value:
                    (self.request if "id" in value else self.notification)(value)
                else:
                    response = self.pending.get(str(value.get("id")))
                    if response is not None:
                        response.put_nowait(value)
        except (ValueError, OSError, KeyError, TypeError, AttributeError, queue.Full):
            pass
        finally:
            for response in list(self.pending.values()):
                try:
                    response.put_nowait({"error": {"code": -32000}})
                except queue.Full:
                    pass
            if not self.closing:
                self.close()

    def close(self):
        self.closing = True
        if self.child.poll() is None:
            try:
                os.killpg(self.child.pid, signal.SIGTERM)
                self.child.wait(timeout=4)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                if self.child.poll() is None:
                    os.killpg(self.child.pid, signal.SIGKILL)
                    self.child.wait(timeout=4)
        self.child.stdin.close()
        self.child.stdout.close()
