"""One private owner per managed harness session; client disconnects do not stop turns."""
import json
import os
import socketserver
import sys
import threading
import uuid
import signal

from kite_harness import atomic, directory, start_ticks, model_ids, owner_lease
from kite_harness_native import LIMIT, Native
from kite_harness_input import native_input
from kite_rpc import TurnCancelled


def text_content(blocks):
    if isinstance(blocks, str):
        return blocks
    return "\n".join(block.get("text", "") for block in blocks or [] if block.get("type") == "text")


class Owner:
    def __init__(self, path):
        self.path = path
        self.info = json.loads((path / "session.json").read_text())
        self.thread = self.info["thread"]
        self.epoch = str(uuid.uuid4())
        self.pending, self.queued, self.active_selection = {}, [], None
        self.cancelled, self.appends = False, 0
        saved = path / "native.json"
        self.saved = json.loads(saved.read_text()) if saved.exists() else None
        self.first_turn = self.saved is None and (path / "history.jsonl").exists()
        self.migration = None
        if self.first_turn:
            history = path / "history.jsonl"
            if history.stat().st_size > 2 * 1024 * 1024:
                raise ValueError("migration context exceeds limit")
            previous = [json.loads(line)["payload"] for line in history.read_text().splitlines()]
            self.migration = json.dumps(previous, ensure_ascii=False)
            if len(self.migration.encode()) > 512 * 1024:
                raise ValueError("migration context exceeds limit")
        self.clients, self.mutex, self.turn = set(), threading.RLock(), None
        self.last_turn = None
        self.revision = 0
        self.state("idle")
        self.native = self.connect_native()

    def persist(self, value):
        self.saved = value
        atomic(self.path / "native.json", value)

    def connect_native(self):
        native = Native(self.info["provider"], self.info["command"], self.info["cwd"], self.info["model"],
                      self.event, self.permission, self.saved, self.persist, route=self.info.get("route"))
        try:
            available = model_ids(self.info["provider"], self.info["model"])
            native.models = [row for row in native.models if json.loads(row["model"])[1] in available]
            if not any(row["model"] == native.current_model for row in native.models):
                raise RuntimeError("active harness model unavailable")
            return native
        except Exception:
            native.close()
            raise

    def permission(self, request):
        with self.mutex:
            identifier = self.epoch + ":permission:" + str(request["id"])
            self.pending[identifier] = request
            self.state("blocked")
            if len(self.pending) == 1:
                self.offer_permission()

    def offer_permission(self):
        if not self.pending:
            return
        identifier, request = next(iter(self.pending.items()))
        params = request.get("params", {})
        tool = params.get("toolCall", {})
        value = {"id": identifier, "method": "item/commandExecution/requestApproval", "params": {
            "threadId": self.thread, "turnId": self.turn, "itemId": tool.get("toolCallId", identifier),
            "command": tool.get("title", "Tool request"), "reason": json.dumps(tool.get("rawInput", {}), ensure_ascii=False),
            "cwd": self.info["cwd"], "availableDecisions": ["accept", "decline"]}}
        for client in list(self.clients):
            client.send(value)

    def response(self, value):
        with self.mutex:
            identifier = value.get("id")
            if not self.pending or identifier != next(iter(self.pending)):
                raise ValueError("approval expired")
            decision = value.get("result", {}).get("decision")
            if decision not in ("accept", "decline"):
                raise ValueError("invalid approval decision")
            self.native.approve(self.pending.pop(identifier), decision == "accept")
            self.notify("serverRequest/resolved", {"requestId": identifier})
            self.state("blocked" if self.pending else "working")
            self.offer_permission()

    def state(self, status):
        self.revision += 1
        if self.turn is not None:
            self.last_turn = self.turn
        atomic(self.path / "state.json", dict(protocol=2, status=status, revision=self.revision, pid=os.getpid(), start_ticks=start_ticks(os.getpid()), turn_id=self.last_turn))

    def notify(self, method, params):
        value = {"method": method, "params": {"threadId": self.thread, **params}}
        for client in list(self.clients):
            client.send(value)

    def record(self, identifier, payload):
        value = dict(id=identifier, type="response_item", payload=payload)
        line = json.dumps(value, ensure_ascii=False) + "\n"
        if len(line.encode()) > LIMIT:
            raise ValueError("harness event exceeds transport limit")
        with self.mutex, (self.path / "history.jsonl").open("a", encoding="utf-8") as output:
            os.chmod(output.name, 0o600)
            output.write(line)
            output.flush()
            os.fsync(output.fileno())

    def message(self, role, text, identifier=None):
        if not text.strip():
            return
        identifier = identifier or str(uuid.uuid4())
        self.record(identifier, {"type": "message", "role": role, "content": [{"type": "text", "text": text}]})
        if role == "assistant":
            self.notify("item/completed", {"turnId": self.turn,
                        "item": {"id": identifier, "type": "agentMessage", "text": text}})

    def tool(self, identifier, name, arguments):
        self.record(identifier, {"type": "function_call", "call_id": identifier,
                                "name": name, "arguments": arguments if isinstance(arguments, str) else json.dumps(arguments)})
        self.notify("item/started", {"turnId": self.turn,
                    "item": {"id": identifier, "type": "commandExecution"}})

    def result(self, identifier, output, failed=False):
        self.record(identifier + ":result", {"type": "function_call_output", "call_id": identifier,
                    "output": output if isinstance(output, str) else json.dumps(output, ensure_ascii=False), "failed": failed})

    def event(self, event):
        data = event.get("data", {})
        kind = event.get("type")
        identifier = self.epoch + ":" + str(event.get("seq", uuid.uuid4()))
        if kind == "assistant/message":
            self.message("assistant", text_content(data.get("message", {}).get("content", [])), identifier)
        elif kind == "tool/call":
            self.tool(str(self.turn) + ":" + str(data["callId"]), data["name"], data.get("arguments", "{}"))
        elif kind == "tool/result":
            message = data.get("message", {})
            self.result(str(self.turn) + ":" + str(message.get("toolCallId", message.get("callId", ""))),
                        text_content(message.get("content", [])), bool(message.get("isError")))

    def finish_turn(self, turn, status, error=None):
        # Caller holds the owner lock: accepting the next input and settling
        # this turn cannot pass each other and silently discard a message.
        self.turn = None
        for identifier in self.pending:
            self.notify("serverRequest/resolved", {"requestId": identifier})
        self.pending.clear()
        self.queued.clear()
        atomic(self.path / "queued.json", self.queued)
        self.state("failed" if status == "failed" else "cancelled" if status == "interrupted" else "done")
        self.notify("turn/completed", {"turn": {"id": turn, "status": status, "error": error}})

    def run(self, blocks, turn, model, effort):
        try:
            while True:
                if self.first_turn:
                    if self.migration:
                        blocks = [{"type": "text", "text": "Earlier public conversation (context only):\n" + self.migration}, *blocks]
                self.first_turn = False
                output = self.native.run(blocks, model, effort)
                if output is not None:
                    self.message("assistant", output)
                with self.mutex:
                    if self.queued and not self.cancelled:
                        blocks, model, effort = self.queued.pop(0)
                        atomic(self.path / "queued.json", self.queued)
                        self.active_selection = (model, effort)
                        self.native.prepare()
                        continue
                    self.finish_turn(turn, "interrupted" if self.cancelled else "completed")
                    return
        except TurnCancelled:
            with self.mutex:
                self.finish_turn(turn, "interrupted")
        except Exception:
            with self.mutex:
                self.finish_turn(turn, "failed", {"message": "Harness turn failed; inspect the conversation before retrying"})

    def ensure_native(self):
        if self.native.child.poll() is not None:
            if self.turn is not None:
                raise RuntimeError("native turn disconnected")
            self.native.close()
            self.native = self.connect_native()

    def request(self, method, params):
        # The same lease protects cold restore and release. An input accepted
        # just before archive must become working before a reaper can inspect it.
        if method in ("initialize", "turn/start"):
            with owner_lease(self.path):
                return self.request_locked(method, params)
        return self.request_locked(method, params)

    def request_locked(self, method, params):
        if method == "initialize":
            with self.mutex:
                self.ensure_native()
            return {"serverInfo": {"name": "kite-harness", "version": "2"}}
        if method == "initialized":
            return {}
        if method == "model/list":
            return {"data": self.native.models, "nextCursor": None}
        if method == "config/read":
            return {"config": {"model": self.native.current_model, "model_reasoning_effort": self.native.current_effort}}
        if method == "account/read":
            return {"account": None, "requiresOpenaiAuth": False}
        if method == "thread/loaded/list":
            return {"data": [self.thread]}
        if params.get("threadId") != self.thread:
            raise ValueError("harness thread mismatch")
        if method == "thread/resume":
            self.offer_permission()
            state = json.loads((self.path / "state.json").read_text())
            if state.get("status") == "failed":
                self.notify("error", {"willRetry": False, "error": {"message": "The previous harness turn failed; inspect history before retrying"}})
                return {"thread": {"id": self.thread, "status": {"type": "systemError"}}}
            return {"thread": {"id": self.thread, "status": {"type": "active" if self.turn else "idle"}}}
        if method == "turn/interrupt":
            with self.mutex:
                if not self.turn or params.get("turnId") != self.turn:
                    raise ValueError("turn expired")
                self.cancelled = True
                self.queued.clear()
                atomic(self.path / "queued.json", self.queued)
                self.native.cancel()
            return {}
        if method == "turn/start":
            model, effort = params.get("model"), params.get("effort", "")
            row = next((m for m in self.native.models if m["model"] == model), None)
            if row is None:
                raise ValueError("model unavailable")
            efforts = [e["reasoningEffort"] for e in row["supportedReasoningEfforts"]]
            if effort not in efforts and not (not efforts and effort == ""):
                raise ValueError("reasoning effort unavailable")
            blocks = native_input(params.get("input"), row)
            prompt = text_content(blocks)
            with self.mutex:
                if self.turn is not None:
                    if self.cancelled or self.appends >= 16:
                        raise ValueError("pending input limit reached")
                    steer = getattr(self.native, "steer", None)
                    delivered = False
                    if steer and self.active_selection == (model, effort) and not self.queued:
                        delivered = steer(blocks)
                    if not delivered:
                        pending = [*self.queued, (blocks, model, effort)]
                        if len(json.dumps(pending).encode()) > 36 * 1024 * 1024:
                            raise ValueError("pending input exceeds limit")
                        self.queued = pending
                        atomic(self.path / "queued.json", self.queued)
                    self.appends += 1
                    self.message("user", prompt)
                    return {"turn": {"id": self.turn, "status": "inProgress"}}
                self.ensure_native()
                self.native.prepare()
                self.active_selection = (model, effort)
                self.cancelled, self.appends = False, 0
                self.turn = turn = str(uuid.uuid4())
                self.state("working")
                self.message("user", prompt, turn + ":user")
                threading.Thread(target=self.run, args=(blocks, turn, model, effort), daemon=True).start()
            return {"turn": {"id": turn, "status": "inProgress"}}
        raise ValueError("unsupported harness operation")


class Handler(socketserver.StreamRequestHandler):
    def setup(self):
        super().setup()
        self.mutex = threading.Lock()
        self.server.owner.clients.add(self)

    def send(self, value):
        try:
            with self.mutex:
                self.wfile.write((json.dumps(value, ensure_ascii=False) + "\n").encode())
                self.wfile.flush()
        except OSError:
            self.server.owner.clients.discard(self)

    def handle(self):
        while line := self.rfile.readline(LIMIT + 1):
            if len(line) > LIMIT or not line.endswith(b"\n"):
                return
            value = {}
            try:
                value = json.loads(line)
                if not isinstance(value, dict):
                    value = {"id": None}
                    raise ValueError("request must be an object")
                if not isinstance(value.get("params", {}), dict):
                    raise ValueError("params must be an object")
                if "method" not in value and "id" in value:
                    self.server.owner.response(value)
                    continue
                result = self.server.owner.request(value["method"], value.get("params", {}))
                response = {"id": value.get("id"), "result": result}
            except (ValueError, KeyError, TypeError):
                response = {"id": value.get("id"), "error": {"code": -32602, "message": "harness request rejected"}}
            except (RuntimeError, OSError):
                response = {"id": value.get("id"), "error": {"code": -32000, "message": "harness backend unavailable"}}
            if "id" in value:
                self.send(response)

    def finish(self):
        self.server.owner.clients.discard(self)
        super().finish()


if __name__ == "__main__":
    os.umask(0o077)
    path = directory(sys.argv[1])
    owner = Owner(path)
    socket = path / "owner.sock"
    socket.unlink(missing_ok=True)
    def stop(_signal, _frame):
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, stop)
    try:
        with socketserver.ThreadingUnixStreamServer(str(socket), Handler) as server:
            os.chmod(socket, 0o600)
            server.daemon_threads = True
            server.owner = owner
            server.serve_forever()
    finally:
        owner.native.close()
        socket.unlink(missing_ok=True)
