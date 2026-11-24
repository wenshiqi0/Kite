import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { array, check, type Json, object, parse, string } from "@kite/protocol";
import { home, lines } from "@kite/runtime";

const nativeFile = join(home(), "agy-fixture-session");
const trace = join(home(), "agy-fixture-methods");
const promptTrace = join(home(), "agy-fixture-prompts");
if (process.argv.includes("-p") && process.argv.includes("/usage")) {
  const mode = process.env.KITE_FIXTURE_AGY_USAGE_MODE || "ready";
  if (mode === "api_key") {
    const message =
      "/usage failed: not available with a Gemini API key; see https://aistudio.google.com/usage for the key's usage";
    process.stderr.write(`error: ${message}\n`);
    process.stdout.write(`${JSON.stringify({ status: "ERROR", error: message })}\n`);
    process.exit(2);
  }
  if (mode === "signed_out") {
    const message = "/usage failed: You are not logged into Antigravity.";
    process.stderr.write(`error: ${message}\n`);
    process.stdout.write(`${JSON.stringify({ status: "ERROR", error: message })}\n`);
    process.exit(2);
  }
  if (mode === "fail") {
    process.stderr.write("error: backend unavailable\n");
    process.exit(1);
  }
  process.stdout.write(
    `${JSON.stringify({
      conversation_id: "",
      status: "SUCCESS",
      num_turns: 0,
      command: {
        name: "usage",
        data: {
          groups: [
            {
              name: "Gemini Models",
              buckets: [
                {
                  id: "gemini-weekly",
                  name: "Weekly Limit Remaining",
                  window: "weekly",
                  remaining_fraction: 0.8247,
                  reset_time: "2026-11-30T08:15:14Z",
                },
                {
                  id: "gemini-5h",
                  name: "Five Hour Limit Remaining",
                  window: "5h",
                  remaining_fraction: 0.3956,
                  reset_time: "2026-11-23T13:15:14Z",
                },
              ],
            },
            {
              name: "Claude and GPT models",
              buckets: [
                {
                  id: "3p-weekly",
                  name: "Weekly Limit Remaining",
                  window: "weekly",
                  remaining_fraction: 1,
                  reset_time: "2026-11-30T12:55:32Z",
                },
                {
                  id: "3p-5h",
                  name: "Five Hour Limit Remaining",
                  window: "5h",
                  reset_time: "2026-11-23T17:55:32Z",
                },
              ],
            },
            {
              name: "Gemini 4 Argon",
              buckets: [
                {
                  id: "gemini-argon-wtu-weekly",
                  name: "Gemini 4 Argon Weekly Limit",
                  window: "weekly",
                  remaining_fraction: 1,
                  reset_time: "2026-11-30T12:55:32Z",
                },
              ],
            },
          ],
        },
      },
    })}\n`,
  );
  process.exit(0);
}
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
    choice("model", "Model", model, ["gemini-pro", "gemini-flash", "claude-opus-5-5"]),
    choice(
      "thought_level",
      "Thinking Level",
      effort,
      model === "gemini-pro"
        ? ["low", "high"]
        : model === "claude-opus-5-5"
          ? ["low", "medium", "high"]
          : ["medium", "high"],
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
const enableLoad = process.env.KITE_FIXTURE_AGY_ENABLE_LOAD === "1";

for await (const line of lines(Bun.stdin.stream(), 40 * 1024 * 1024)) {
  const value = object(parse(line)),
    p = object(value.params ?? {});
  if (typeof value.method === "string") appendFileSync(trace, `${value.method}\n`);
  switch (value.method) {
    case "initialize":
      reply(value.id, {
        protocolVersion: 1,
        agentInfo: { name: "Antigravity ACP Server", version: "1.5.16" },
        agentCapabilities: {
          ...(enableLoad ? { loadSession: true } : {}),
          promptCapabilities: { image: true },
          sessionCapabilities: {
            list: {},
            resume: {},
            ...(enableLoad ? { load: {}, close: {} } : {}),
          },
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
          sessions:
            existsSync(nativeFile) && process.env.KITE_FIXTURE_AGY_OMIT_FROM_LIST !== "1"
              ? [{ sessionId: readFileSync(nativeFile, "utf8") }]
              : [],
        });
      break;
    case "session/load":
      if (!existsSync(nativeFile) || readFileSync(nativeFile, "utf8") !== string(p.sessionId)) {
        send({
          jsonrpc: "2.0",
          id: value.id,
          error: { code: -32002, message: `Session not found: ${string(p.sessionId)}` },
        });
        break;
      }
      sid = string(p.sessionId);
      update({
        sessionUpdate: "agent_message_chunk",
        content: { type: "text", text: "REPLAYED_HISTORY_SHOULD_NOT_EMIT" },
      });
      reply(value.id, {
        configOptions: configOptions(),
        models: {
          availableModels: [
            { modelId: "gemini-pro-high", name: "gemini-pro-high" },
            { modelId: "gemini-pro-low", name: "gemini-pro-low" },
            { modelId: "gemini-flash-medium", name: "gemini-flash-medium" },
            { modelId: "gemini-flash-high", name: "gemini-flash-high" },
            { modelId: "claude-opus-5-5-low", name: "Claude Opus 5.5 (Low)" },
            { modelId: "claude-opus-5-5-medium", name: "Claude Opus 5.5 (Medium)" },
            { modelId: "claude-opus-5-5-high", name: "Claude Opus 5.5 (High)" },
          ],
        },
        _meta: { antigravity: { tier: { name: "Google AI Ultra", id: "g1-ultra-tier" } } },
      });
      update({
        sessionUpdate: "available_commands_update",
        availableCommands: [
          { name: "compact", description: "Compact the conversation" },
          { name: "kite", description: "Operate a Kite host" },
        ],
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
            { modelId: "gemini-pro-high", name: "gemini-pro-high" },
            { modelId: "gemini-pro-low", name: "gemini-pro-low" },
            { modelId: "gemini-flash-medium", name: "gemini-flash-medium" },
            { modelId: "gemini-flash-high", name: "gemini-flash-high" },
            { modelId: "claude-opus-5-5-low", name: "Claude Opus 5.5 (Low)" },
            { modelId: "claude-opus-5-5-medium", name: "Claude Opus 5.5 (Medium)" },
            { modelId: "claude-opus-5-5-high", name: "Claude Opus 5.5 (High)" },
          ],
        },
        _meta: { antigravity: { tier: { name: "Google AI Ultra", id: "g1-ultra-tier" } } },
      });
      update({
        sessionUpdate: "available_commands_update",
        availableCommands: [
          { name: "compact", description: "Compact the conversation" },
          { name: "kite", description: "Operate a Kite host" },
        ],
      });
      break;
    case "session/close":
      reply(value.id, {});
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
      const promptBlocks = array(p.prompt);
      appendFileSync(promptTrace, `${JSON.stringify(promptBlocks)}\n`);
      const prompt = object(promptBlocks[0]);
      update({
        sessionUpdate: "agent_thought_chunk",
        content: { type: "text", text: "PRIVATE_FIXTURE_THOUGHT" },
      });
      if (prompt.text === "diff") {
        const tool = {
          toolCallId: "fixture-diff-tool",
          title: "Edit README.md\nsecond line should not bloat tool name",
          kind: "edit",
          rawInput: { Path: "README.md" },
        };
        update({ sessionUpdate: "tool_call", status: "in_progress", ...tool });
        update({
          sessionUpdate: "tool_call_update",
          toolCallId: tool.toolCallId,
          status: "completed",
          content: [{ type: "diff", path: "README.md", oldText: "before", newText: "after" }],
          rawOutput: "Edited README.md",
        });
        update({
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "Diff applied" },
        });
        writeFileSync(nativeFile, sid, { mode: 0o600 });
        reply(value.id, { stopReason: "end_turn" });
      } else if (prompt.text === "approve") {
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
            rawOutput: "Turbo tool ran",
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
        rawOutput: { exitCode: 1 },
      });
      update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Con" } });
      update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "tinued" } });
      writeFileSync(nativeFile, sid, { mode: 0o600 });
      reply(pending, { stopReason: "end_turn" });
      pending = undefined;
      break;
  }
}
