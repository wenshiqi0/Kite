"""Scripted DSH SDK peer; no provider, credentials or external tools."""
import json
import sys
import time

mode = sys.argv[1]
number = 0


def emit(value):
    print(json.dumps(value), flush=True)


for line in sys.stdin:
    value = json.loads(line)
    method, params = value.get("method"), value.get("params", {})
    result = {}
    if method == "initialize":
        result = {"serverInfo": {"name": "deepseek-harness-sdk-runtime" if mode == "dsh" else mode, "version": "fixture"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "agent", "inputSchema": {"type": "object"}}]}
    elif method == "session/prompt":
        session = params["sessionId"]
        number += 1
        emit({"method": "session.status", "params": {"sessionId": session, "status": "idle"}})
        emit({"id": value["id"], "result": {"messageId": str(number)}})
        emit({"method": "session.status", "params": {"sessionId": session, "status": "running"}})
        time.sleep(.25)
        events = [
            {"type": "tool/call", "data": {"callId": str(number), "name": "read", "arguments": "{}"}},
            {"type": "tool/result", "data": {"message": {"toolCallId": str(number), "content": [{"type": "text", "text": "owned"}]}}},
            {"type": "assistant/message", "data": {"message": {"content": [{"type": "text", "text": "DSH reply " + str(number)}]}}},
            {"type": "turn/end", "data": {"reason": {"kind": "completed"}}},
        ]
        for seq, event in enumerate(events):
            event["seq"] = number * 10 + seq
            emit({"method": "session.event", "params": {"sessionId": session, "event": event}})
        emit({"method": "session.status", "params": {"sessionId": session, "status": "idle"}})
        continue
    elif method == "shutdown":
        emit({"id": value["id"], "result": {}})
        break
    if "id" in value:
        emit({"id": value["id"], "result": result})
