"""Protocol fixtures for ACP and Ply host MCP; never make a model request."""
import json
from pathlib import Path
import sys
import time

mode = sys.argv[1]
route = "deepseek-official"
model = "deepseek-v4-flash"
number = 0
session = "native-session"
effort = "high"


def emit(value):
    print(json.dumps(value), flush=True)


def options():
    return [{"id": "model", "currentValue": json.dumps([route, model], separators=(",", ":"))},
            {"id": "reasoning_effort", "currentValue": effort}]


def update(value):
    emit({"method": "session/update", "params": {"sessionId": session, "update": value}})


if "--patch" in sys.argv:
    patch = json.loads(Path(sys.argv[sys.argv.index("--patch") + 1]).read_text())
    config = patch[0]["config"]
    route, model = config["provider"], config["model"]
    rows = [dict(model=json.dumps([route, name], separators=(",", ":")), displayName=name, description="",
        hidden=False, isDefault=name == model, inputModalities=["text", "image"] if name == "deepseek-flash" else ["text"],
        supportedReasoningEfforts=[dict(reasoningEffort=e, description=e) for e in ["off", "low", "high", "max"]], defaultReasoningEffort="high")
        for name in ["deepseek-v4-flash", "deepseek-flash"]]
    Path(patch[1]["insert"][0]["config"]["output"]).write_text(json.dumps({"models": rows}))

for line in sys.stdin:
    value = json.loads(line)
    method, params = value.get("method"), value.get("params", {})
    result = {}
    if method == "initialize":
        if "--patch" in sys.argv:
            result = {"agentInfo": {"name": "deepseek-harness-acp" if mode == "dsh" else mode},
                      "agentCapabilities": {"promptCapabilities": {"image": model == "deepseek-flash"}}}
        else:
            result = {"serverInfo": {"name": "ply" if mode == "ply" else mode}, "capabilities": {"experimental": {"plyAgent": 1}}}
    elif method in ("session/new", "session/resume"):
        result = {"sessionId": session, "configOptions": options()}
    elif method == "session/set_config_option":
        if params["configId"] == "model":
            route, model = json.loads(params["value"])
        else:
            effort = params["value"]
        result = {"configOptions": options()}
    elif method == "session/prompt":
        number += 1
        prompt = "\n".join(b.get("text", "") for b in params["prompt"])
        if "approval-fixture" in prompt:
            emit({"id": "permission-1", "method": "session/request_permission", "params": {"sessionId": session,
                "toolCall": {"toolCallId": "read-1", "title": "read owned file", "rawInput": {"path": "owned"}},
                "options": [{"optionId": "yes", "kind": "allow_once"}, {"optionId": "no", "kind": "reject_once"}]}})
            decision = json.loads(sys.stdin.readline())
            if decision.get("result", {}).get("outcome", {}).get("optionId") != "yes":
                emit({"id": value["id"], "result": {"stopReason": "cancelled"}})
                continue
        if "cancel-fixture" in prompt:
            cancellation = json.loads(sys.stdin.readline())
            assert cancellation["method"] == "session/cancel"
            emit({"id": value["id"], "result": {"stopReason": "cancelled"}})
            continue
        time.sleep(.08)
        update({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "PRIVATE REASONING MUST NOT APPEAR"}})
        update({"sessionUpdate": "tool_call", "toolCallId": str(number), "title": "read", "rawInput": {"path": "owned"}})
        update({"sessionUpdate": "tool_call_update", "toolCallId": str(number), "status": "completed", "content": [{"type": "content", "content": {"type": "text", "text": "owned"}}]})
        text = "DSH reply " + str(number) + " " + model + " " + effort
        for block in params["prompt"]:
            if block["type"] == "image":
                assert "path" not in block and block["data"]
                text += " native-vision:" + block["mimeType"]
        update({"sessionUpdate": "agent_message_chunk", "messageId": str(number), "content": {"type": "text", "text": text}})
        result = {"stopReason": "end_turn"}
    elif method == "ply/agent/catalog":
        result = [{"id": '["deepseek","deepseek-v4-flash"]', "name": "DeepSeek V4 Flash", "input": ["text", "image"],
                   "efforts": ["off", "high", "max"], "defaultEffort": "high"}]
    elif method == "ply/agent/chat":
        time.sleep(.15)
        result = {"result": {"text": "Ply reply " + params["effort"]}, "snapshot": {"conversation": "owned"}}
    elif method == "ply/agent/state":
        result = {"pending": []}
    if "id" in value:
        emit({"id": value["id"], "result": result})
