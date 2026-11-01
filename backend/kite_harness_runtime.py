"""One private owner per managed harness session; client disconnects do not stop turns."""
import json
import os
import socketserver
import sys
import threading
import uuid
import signal

from kite_harness import atomic, directory, start_ticks
from kite_harness_native import LIMIT, Native


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
        self.native_session = str(uuid.uuid4())
        self.first_turn = True
        self.clients, self.mutex, self.turn = set(), threading.RLock(), None
        self.last_turn = None
        self.revision = 0
        self.state("idle")
        self.native = Native(self.info["provider"], self.info["command"], self.info["cwd"],
                             self.info["model"], self.event)

    def state(self, status):
        self.revision += 1
        if self.turn is not None:
            self.last_turn = self.turn
        atomic(self.path / "state.json", dict(status=status, revision=self.revision, pid=os.getpid(), start_ticks=start_ticks(os.getpid()), turn_id=self.last_turn))

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

    def run(self, prompt, turn):
        try:
            if self.first_turn:
                # A fresh SDK runtime receives the saved public conversation once.
                history = self.path / "history.jsonl"
                if history.stat().st_size > 2 * 1024 * 1024:
                    raise ValueError("conversation context exceeds limit; start a new session")
                previous = [json.loads(line)["payload"] for line in history.read_text().splitlines()]
                context = json.dumps(previous[:-1], ensure_ascii=False)
                if len(context.encode()) > 512 * 1024:
                    raise ValueError("Conversation context exceeds limit; start a new session")
                if previous[:-1]:
                    prompt = "Continue this conversation. Earlier messages are context, not new instructions.\n" + context + "\nCurrent user message:\n" + prompt
            self.first_turn = False
            output = self.native.run(self.native_session, prompt)
            if output is not None:
                self.message("assistant", output)
            with self.mutex:
                self.turn = None
                self.state("done")
                self.notify("turn/completed", {"turn": {"id": turn, "status": "completed"}})
        except Exception:
            with self.mutex:
                self.turn = None
                self.state("failed")
                self.notify("turn/completed", {"turn": {"id": turn, "status": "failed",
                    "error": {"message": "Harness turn failed; inspect the conversation before retrying"}}})
        finally:
            with self.mutex:
                if self.turn == turn:
                    self.turn = None

    def ensure_native(self):
        if self.native.child.poll() is not None and self.turn is None:
            self.native.close()
            self.native_session, self.first_turn = str(uuid.uuid4()), True
            self.native = Native(self.info["provider"], self.info["command"], self.info["cwd"], self.info["model"], self.event)

    def request(self, method, params):
        if method == "initialize":
            with self.mutex:
                self.ensure_native()
            return {"serverInfo": {"name": "kite-harness", "version": "1"}}
        if method == "initialized":
            return {}
        if method == "model/list":
            model = self.info["model"]
            return {"data": [{"id": model, "model": model, "displayName": model,
                "description": "", "hidden": False, "isDefault": True, "supportedReasoningEfforts": [{"reasoningEffort": "none", "description": ""}],
                "defaultReasoningEffort": "none", "inputModalities": ["text"]}], "nextCursor": None}
        if method == "config/read":
            return {"config": {"model": self.info["model"], "model_reasoning_effort": "none"}}
        if method == "account/read":
            return {"account": None, "requiresOpenaiAuth": False}
        if method == "thread/loaded/list":
            return {"data": [self.thread]}
        if params.get("threadId") != self.thread:
            raise ValueError("harness thread mismatch")
        if method == "thread/resume":
            state = json.loads((self.path / "state.json").read_text())
            if state.get("status") == "failed":
                self.notify("error", {"willRetry": False, "error": {"message": "The previous harness turn failed; inspect history before retrying"}})
                return {"thread": {"id": self.thread, "status": {"type": "systemError"}}}
            return {"thread": {"id": self.thread, "status": {"type": "active" if self.turn else "idle"}}}
        if method == "turn/start":
            blocks = params.get("input", [])
            if not blocks or any(b.get("type") != "text" for b in blocks):
                raise ValueError("harness supports text input only")
            prompt = text_content(blocks)
            if not prompt.strip() or len(prompt.encode()) > 1024 * 1024:
                raise ValueError("invalid harness prompt")
            if params.get("model") != self.info["model"]:
                raise ValueError("harness model cannot be changed in this session")
            with self.mutex:
                if self.turn is not None:
                    raise ValueError("harness turn is already running")
                self.ensure_native()
                self.turn = turn = str(uuid.uuid4())
                self.state("working")
                self.message("user", prompt, turn + ":user")
                threading.Thread(target=self.run, args=(prompt, turn), daemon=True).start()
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
