import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { array, check, type Json, object, parse, string } from "@kite/protocol";
import { home, lines } from "@kite/runtime";

const nativeFile = join(home(), "agy-fixture-session");
const trace = join(home(), "agy-fixture-methods");
const turbo = process.argv.includes("--dangerously-skip-permissions");
appendFileSync(
  join(home(), "agy-fixture-launches"),
  `${JSON.stringify({ pid: process.pid, turbo })}\n`,
);
const send = (value: unknown) => process.stdout.write(`${JSON.stringify(value)}\n`);
const choice = (id: string, name: string, currentValue: string, options: string[]) => ({
  id,
  name,
  type: "select",
  category: id,
  currentValue,
  options: options.map((value) => ({ value, name: value })),
});
let sid = "",
  model = "gemini-pro",
  effort = "low",
  mode = "default";
let pending: Json | undefined;
function configOptions() {
  return [
    choice("model", "Model", model, ["gemini-pro", "gemini-flash"]),
    choice(
      "thought_level",
      "Thinking Level",
      effort,
      model === "gemini-pro" ? ["low", "high"] : ["medium", "high"],
    ),
    choice("mode", "Mode", mode, ["default", "accept-edits", "plan"]),
  ];
}
function reply(id: Json | undefined, result: Json) {
  send({ jsonrpc: "2.0", id, result });
}
function update(value: Json) {
  send({ jsonrpc: "2.0", method: "session/update", params: { sessionId: sid, update: value } });
}

for await (const line of lines(Bun.stdin.stream(), 40 * 1024 * 1024)) {
  const value = object(parse(line)),
    p = object(value.params ?? {});
  if (typeof value.method === "string") appendFileSync(trace, `${value.method}\n`);
  switch (value.method) {
    case "initialize":
      reply(value.id, {
        protocolVersion: 1,
        agentInfo: { name: "Antigravity ACP Server", version: "1.5.14" },
        agentCapabilities: {
          promptCapabilities: { image: true },
          sessionCapabilities: { list: {}, resume: {} },
        },
      });
      break;
    case "session/list":
      if (p.cwd === home() && process.env.KITE_FIXTURE_AGY_SLOW_LOGIN === "1")
        await Bun.sleep(4200);
      if (process.env.KITE_FIXTURE_AGY_SIGNED_IN !== "1")
        send({
          jsonrpc: "2.0",
          id: value.id,
          error: { code: -32000, message: "Authentication required" },
        });
      else
        reply(value.id, {
          sessions: existsSync(nativeFile) ? [{ sessionId: readFileSync(nativeFile, "utf8") }] : [],
        });
      break;
    case "session/resume":
    case "session/new":
      sid = value.method === "session/resume" ? string(p.sessionId) : crypto.randomUUID();
      reply(value.id, {
        sessionId: sid,
        configOptions: configOptions(),
        models: {
          availableModels: [
            "gemini-pro-high",
            "gemini-pro-low",
            "gemini-flash-medium",
            "gemini-flash-high",
          ].map((modelId) => ({ modelId, name: modelId })),
        },
      });
      break;
    case "session/set_config_option":
      check(p.sessionId === sid, "fixture session mismatch");
      if (p.configId === "model") {
        model = string(p.value);
        effort = model === "gemini-pro" ? "low" : "medium";
      } else if (p.configId === "thought_level") effort = string(p.value);
      else if (p.configId === "mode") mode = string(p.value);
      reply(value.id, { configOptions: configOptions() });
      break;
    case "session/prompt": {
      check(p.sessionId === sid, "fixture session mismatch");
      const prompt = object(array(p.prompt)[0]);
      update({
        sessionUpdate: "agent_thought_chunk",
        content: { type: "text", text: "PRIVATE_FIXTURE_THOUGHT" },
      });
      if (prompt.text === "approve") {
        const tool = {
          toolCallId: "fixture-tool",
          title: "Print fixture output",
          kind: "execute",
          rawInput: { CommandLine: "printf kite-fixture" },
        };
        update({
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "Checking" },
        });
        update({ sessionUpdate: "tool_call", status: "in_progress", ...tool });
        if (turbo) {
          update({
            sessionUpdate: "tool_call_update",
            toolCallId: tool.toolCallId,
            status: "completed",
            content: [{ type: "content", content: { type: "text", text: "Turbo tool ran" } }],
          });
          writeFileSync(nativeFile, sid, { mode: 0o600 });
          reply(value.id, { stopReason: "end_turn" });
        } else {
          pending = value.id;
          send({
            jsonrpc: "2.0",
            id: "approve-tool",
            method: "session/request_permission",
            params: {
              sessionId: sid,
              toolCall: { toolCallId: tool.toolCallId },
              options: [
                { kind: "allow_once", optionId: "allow" },
                { kind: "reject_once", optionId: "deny" },
              ],
            },
          });
        }
      } else if (prompt.text === "cancel") pending = value.id;
      else {
        update({
          sessionUpdate: "agent_message_chunk",
          content: {
            type: "text",
            text: `Restored ${existsSync(nativeFile)} ${model} ${effort} ${mode}`,
          },
        });
        reply(value.id, { stopReason: "end_turn" });
      }
      break;
    }
    case "session/cancel":
      if (pending !== undefined) reply(pending, { stopReason: "cancelled" });
      pending = undefined;
      break;
    default:
      if (value.id !== "approve-tool" || pending === undefined) break;
      check(
        object(object(value.result).outcome).optionId === "deny",
        "fixture permission was not rejected",
      );
      update({
        sessionUpdate: "tool_call_update",
        toolCallId: "fixture-tool",
        status: "failed",
        content: [{ type: "content", content: { type: "text", text: "Denied once" } }],
      });
      update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Con" } });
      update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "tinued" } });
      writeFileSync(nativeFile, sid, { mode: 0o600 });
      reply(pending, { stopReason: "end_turn" });
      pending = undefined;
      break;
  }
}
