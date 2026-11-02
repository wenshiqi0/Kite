"""Ply Linux/macOS native MCP host chat, never a parallel model execution loop."""
import json
from pathlib import Path
import tempfile
import threading

from kite_rpc import RpcProcess, RpcError, TurnCancelled


class Ply:
    def __init__(self, command, cwd, model, event, permission, saved=None, persist=lambda value: None, inspect=False):
        self.event, self.permission, self.persist, self.snapshot = event, permission, persist, saved
        self.workspace = tempfile.TemporaryDirectory(prefix="kite-ply-")
        self.record = Path(self.workspace.name) / "events.jsonl"
        self.cursor, self.calls = 0, set()
        self.request_id, self.cancelled = None, threading.Event()
        self.rpc = RpcProcess([*command, "--record", str(self.record)], cwd)
        try:
            hello = self.rpc.call("initialize", {"protocolVersion": "2025-06-18", "clientInfo": {"name": "kite", "version": "2"}, "capabilities": {}}, 15)
            if hello.get("serverInfo", {}).get("name") != "ply" or hello.get("capabilities", {}).get("experimental", {}).get("plyAgent") != 1:
                raise RuntimeError("Ply host chat capability unavailable")
            self.rpc.write({"method": "notifications/initialized"})
            rows = self.rpc.call("ply/agent/catalog", {}, 15)
            self.models = []
            for row in rows:
                self.models.append(dict(model=row["id"], displayName=row["name"], description="", hidden=False,
                    isDefault=json.loads(row["id"])[1] == model, inputModalities=row["input"],
                    supportedReasoningEfforts=[dict(reasoningEffort=e, description=e) for e in row["efforts"]],
                    defaultReasoningEffort=row["defaultEffort"]))
            selected = next((r for r in self.models if r["isDefault"]), None)
            if selected is None or len(self.models) > 512:
                raise RuntimeError("Ply model catalog unavailable")
            self.current_model, self.current_effort = selected["model"], selected["defaultReasoningEffort"]
        except Exception:
            self.close()
            raise

    @property
    def child(self):
        return self.rpc.child

    def prepare(self):
        self.cancelled.clear()
        self.request_id = None

    def run(self, blocks, model, effort):
        stop, seen = threading.Event(), set()
        def poll():
            while not stop.wait(.15):
                try:
                    self.events()
                    state = self.rpc.call("ply/agent/state", {})
                    for request in state.get("pending", []):
                        if request["id"] not in seen:
                            seen.add(request["id"])
                            self.permission({"id": request["id"], "params": {"toolCall": {
                                "toolCallId": request["id"], "title": request.get("command", request["tool"]),
                                "rawInput": request.get("input", {})}}})
                except (RuntimeError, OSError):
                    return
        watcher = threading.Thread(target=poll, daemon=True)
        watcher.start()
        try:
            value = self.rpc.call("ply/agent/chat", dict(prompt="\n".join(b["text"] for b in blocks if b["type"] == "text"),
                images=[b for b in blocks if b["type"] == "image"], model=model, effort=effort,
                snapshot=self.snapshot), 3600, on_id=self.running)
            result = value["result"]
            if result.get("status") == "approval_required" or not result.get("text", "").strip():
                raise RuntimeError("Ply did not complete its turn")
            self.snapshot = value["snapshot"]
            self.persist(self.snapshot)
            self.current_model, self.current_effort = model, effort
            return result["text"]
        except RpcError as error:
            if error.code == -32800:
                raise TurnCancelled() from None
            raise
        finally:
            stop.set()
            watcher.join(timeout=2)
            self.events()
            self.request_id = None

    def events(self):
        if not self.record.exists():
            return
        with self.record.open("rb") as stream:
            stream.seek(self.cursor)
            while line := stream.readline(40 * 1024 * 1024 + 1):
                if not line.endswith(b"\n") or len(line) > 40 * 1024 * 1024:
                    return
                self.cursor = stream.tell()
                value = json.loads(line)
                if value.get("event") == "spawn":
                    fiber = value["fiber"]
                    if fiber.get("plugin") in ("bash", "read", "edit", "create", "remove", "skill", "agent", "memory"):
                        self.calls.add(fiber["id"])
                        self.event({"type": "tool/call", "data": {"callId": fiber["id"], "name": fiber["plugin"], "arguments": json.dumps(fiber.get("input", {}), ensure_ascii=False)}})
                elif value.get("event") == "exit" and value.get("id") in self.calls:
                    self.calls.remove(value["id"])
                    result = value["exit"]
                    self.event({"type": "tool/result", "data": {"message": {"toolCallId": value["id"],
                        "content": [{"type": "text", "text": json.dumps(result.get("output", {"status": result["status"]}), ensure_ascii=False)}],
                        "isError": result["status"] != "done"}}})

    def running(self, identifier):
        self.request_id = identifier
        if self.cancelled.is_set():
            self.cancel()

    def cancel(self):
        self.cancelled.set()
        if self.request_id is not None:
            self.rpc.write({"method": "notifications/cancelled", "params": {"requestId": self.request_id}})

    def approve(self, request, accepted):
        self.rpc.call("ply/agent/approve", {"id": request["id"], "accepted": accepted})

    def close(self):
        self.rpc.close()
        self.workspace.cleanup()
