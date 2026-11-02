"""DSH ACP controls and native catalog; no guessed model capabilities."""
import json
import socket
from pathlib import Path
import tempfile
import time
import threading
import uuid

from kite_rpc import RpcProcess, TurnCancelled


class Dsh:
    def __init__(self, command, cwd, route, model, event, permission, saved=None, persist=lambda value: None, inspect=False):
        self.command, self.cwd, self.event, self.permission = command, cwd, event, permission
        self.persist, self.session, self.text, self.tools = persist, None, {}, {}
        self.cancelled = threading.Event()
        self.workspace = tempfile.TemporaryDirectory(prefix="kite-acp-")
        self.path = Path(self.workspace.name)
        self.rpc = None
        try:
            self._start(route, model)
            if not inspect:
                if saved:
                    value = self.rpc.call("session/resume", {"sessionId": saved, "cwd": cwd, "mcpServers": []})
                    self.session = saved
                else:
                    value = self.rpc.call("session/new", {"cwd": cwd, "mcpServers": []})
                    self.session = value["sessionId"]
                self.config = value.get("configOptions", [])
                self.persist(self.session)
            else:
                self.config = []
            self.current_model = next((o["currentValue"] for o in self.config if o["id"] == "model"), json.dumps([route, model], separators=(",", ":")))
            self.current_effort = next((o["currentValue"] for o in self.config if o["id"] == "reasoning_effort"), "")
            for row in self.models:
                row["isDefault"] = row["model"] == self.current_model

        except Exception:
            self.close()
            raise

    @property
    def child(self):
        return self.rpc.child

    def _start(self, route, model):
        output = self.path / (uuid.uuid4().hex + ".json")
        patch = self.path / "profile.patch.json"
        patch.write_text(json.dumps([
            {"id": "acp", "config": {"provider": route, "model": model}},
            {"insert": [{"id": "kite-catalog", "name": str(Path(__file__).with_name("kite_dsh_catalog.mjs")), "config": {"output": str(output), "provider": route, "control": str(self.path / "control.sock")}}]},
        ]))
        (self.path / "control.sock").unlink(missing_ok=True)
        self.rpc = RpcProcess([*self.command, "--patch", str(patch)], self.cwd, self.update, self.request)
        try:
            # ACP's optional attachment service may finish loading after its
            # transport starts. Wait for the native loader before negotiation.
            for _ in range(400):
                if output.exists():
                    break
                if self.rpc.child.poll() is not None:
                    raise RuntimeError("DSH exited during startup")
                time.sleep(.05)
            hello = self.rpc.call("initialize", {"protocolVersion": 1, "clientInfo": {"name": "kite", "version": "2"}, "clientCapabilities": {}}, 15)
            if hello.get("agentInfo", {}).get("name") != "deepseek-harness-acp":
                raise RuntimeError("DSH ACP identity mismatch")
            self.image = hello.get("agentCapabilities", {}).get("promptCapabilities", {}).get("image") is True
            self.models = json.loads(output.read_text())["models"]
            if not self.models or len(self.models) > 512:
                raise ValueError("invalid DSH model catalog")
        except Exception:
            self.rpc.close()
            raise

    def select(self, model, effort):
        row = next((r for r in self.models if r["model"] == model), None)
        if row is None:
            raise ValueError("model unavailable")
        choices = [e["reasoningEffort"] for e in row["supportedReasoningEfforts"]]
        if effort not in choices and not (not choices and effort == ""):
            raise ValueError("reasoning effort unavailable")
        if "image" in row["inputModalities"] and not self.image:
            # ACP advertises images at connection initialization. Renegotiate at
            # an idle boundary and resume the same native persisted session.
            saved = self.session
            self.rpc.call("session/close", {"sessionId": saved})
            self.rpc.close()
            route, native_model = json.loads(model)
            self._start(route, native_model)
            value = self.rpc.call("session/resume", {"sessionId": saved, "cwd": self.cwd, "mcpServers": []})
            self.config = value.get("configOptions", [])
            self.current_model = next((o["currentValue"] for o in self.config if o["id"] == "model"), model)
            self.session = saved
        if model != self.current_model:
            result = self.rpc.call("session/set_config_option", {"sessionId": self.session, "configId": "model", "value": model})
            self.config = result["configOptions"]
            self.current_model = model
        option = next((o for o in self.config if o["id"] == "reasoning_effort"), None)
        if option is not None:
            result = self.rpc.call("session/set_config_option", {"sessionId": self.session, "configId": "reasoning_effort", "value": effort})
            self.config = result["configOptions"]
        self.current_effort = effort

    def steer(self, blocks):
        if any(block["type"] != "text" for block in blocks):
            return False
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(5)
            try:
                connection.connect(str(self.path / "control.sock"))
            except (FileNotFoundError, ConnectionRefusedError):
                return False
            connection.sendall((json.dumps({"method": "steer", "sessionId": self.session,
                "text": "\n".join(block["text"] for block in blocks)}) + "\n").encode())
            with connection.makefile("rb") as stream:
                value = json.loads(stream.readline(4096))
            if "error" in value:
                raise RuntimeError("native steering failed; delivery may have occurred")
            return value.get("accepted") is True

    def prepare(self):
        self.cancelled.clear()

    def run(self, blocks, model, effort):
        if self.cancelled.is_set():
            raise TurnCancelled()
        self.select(model, effort)
        if self.cancelled.is_set():
            raise TurnCancelled()
        self.text.clear()
        self.tools.clear()
        value = self.rpc.call("session/prompt", {"sessionId": self.session, "prompt": blocks}, 3600, on_id=lambda _: self.cancel() if self.cancelled.is_set() else None)
        if value.get("stopReason") == "cancelled":
            raise TurnCancelled()
        if value.get("stopReason") != "end_turn":
            raise RuntimeError("DSH turn failed")

    def cancel(self):
        self.cancelled.set()
        self.rpc.write({"method": "session/cancel", "params": {"sessionId": self.session}})

    def update(self, value):
        if value.get("method") != "session/update":
            return
        params = value.get("params", {})
        if params.get("sessionId") != self.session:
            return
        update = params.get("update", {})
        kind = update.get("sessionUpdate")
        if kind == "agent_message_chunk" and update.get("content", {}).get("type") == "text":
            identifier = update.get("messageId") or str(uuid.uuid4())
            self.text[identifier] = self.text.get(identifier, "") + update["content"]["text"]
            self.event({"type": "assistant/message", "seq": identifier, "data": {"message": {"content": [{"type": "text", "text": self.text[identifier]}]}}})
        elif kind == "tool_call":
            self.tools[update["toolCallId"]] = update
            self.event({"type": "tool/call", "data": {"callId": update["toolCallId"], "name": update.get("title", "tool"), "arguments": json.dumps(update.get("rawInput", {}))}})
        elif kind == "tool_call_update" and update.get("status") in ("completed", "failed"):
            output = "\n".join(c.get("content", {}).get("text", "") for c in update.get("content", []) if c.get("type") == "content")
            self.event({"type": "tool/result", "data": {"message": {"toolCallId": update["toolCallId"], "content": [{"type": "text", "text": output}], "isError": update["status"] == "failed"}}})

    def request(self, value):
        if value.get("method") == "session/request_permission":
            params = value.get("params", {})
            tool = params.get("toolCall", {})
            params["toolCall"] = {**self.tools.get(tool.get("toolCallId"), {}), **tool}
            self.permission(value)
        else:
            self.rpc.write({"id": value.get("id"), "error": {"code": -32601, "message": "unsupported native request"}})

    def approve(self, request, accepted):
        options = request.get("params", {}).get("options", [])
        chosen = next((o for o in options if o.get("kind") == ("allow_once" if accepted else "reject_once")), None)
        outcome = {"outcome": "selected", "optionId": chosen["optionId"]} if chosen else {"outcome": "cancelled"}
        self.rpc.write({"id": request["id"], "result": {"outcome": outcome}})

    def close(self):
        if self.rpc is not None:
            self.rpc.close()
        self.workspace.cleanup()
