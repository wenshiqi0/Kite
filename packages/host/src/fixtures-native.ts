/** Deterministic native protocol peer; never reaches a real model or credentials. */
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { array, check, type Json, type ObjectValue, object, parse, string } from "@kite/protocol";
import { lines } from "@kite/runtime";

const mode = process.argv[2] || "ply",
  model = "fixture-model",
  route = "fixture",
  id = JSON.stringify([route, model]);
let session = "fixture-native-session",
  effort = "high";
let counter = 0;
let record = "";
const pending = new Map<string, { resolve: () => void; reject: () => void }>();
let active: number | string | null = null;
for (const flag of ["--record", "--patch"]) {
  const at = process.argv.indexOf(flag);
  if (at < 0) continue;
  const path = process.argv[at + 1];
  check(path, "fixture path missing");
  if (flag === "--record") record = path;
  else {
    const patch = array(parse(readFileSync(path, "utf8")));
    const config = object(object(array(object(patch[1]).insert)[0]).config);
    writeFileSync(
      string(config.output),
      JSON.stringify({
        models: [
          {
            model: id,
            displayName: "Fixture native model",
            description: "",
            hidden: false,
            isDefault: true,
            inputModalities: ["text", "image"],
            supportedReasoningEfforts: [
              { reasoningEffort: "off", description: "Off" },
              { reasoningEffort: "high", description: "High" },
            ],
            defaultReasoningEffort: "high",
          },
        ],
      }),
    );
  }
}
const send = (value: unknown) => process.stdout.write(`${JSON.stringify(value)}\n`);
const config = () => [
  { id: "model", currentValue: id },
  { id: "reasoning_effort", currentValue: effort },
];
async function request(method: string, p: ObjectValue, rid: string | number): Promise<Json> {
  if (method === "initialize")
    return mode === "dsh"
      ? {
          agentInfo: { name: "deepseek-harness-acp" },
          agentCapabilities: { promptCapabilities: { image: true } },
        }
      : { serverInfo: { name: "ply" }, capabilities: { experimental: { plyAgent: 1 } } };
  if (method === "session/new" || method === "session/resume") {
    session = typeof p.sessionId === "string" ? p.sessionId : session;
    return { sessionId: session, configOptions: config() };
  }
  if (method === "session/set_config_option") {
    if (p.configId === "reasoning_effort") effort = string(p.value);
    return { configOptions: config() };
  }
  if (method === "ply/agent/catalog")
    return [
      {
        id,
        name: "Fixture native model",
        input: ["text", "image"],
        efforts: ["off", "high"],
        defaultEffort: "high",
      },
    ];
  if (method === "ply/agent/state")
    return {
      pending: [...pending.keys()].map((id) => ({
        id,
        tool: "bash",
        command: "fixture owned operation",
        input: {},
      })),
    };
  if (method === "ply/agent/approve") {
    pending.get(String(p.id))?.resolve();
    pending.delete(String(p.id));
    return {};
  }
  if (method === "session/prompt" || method === "ply/agent/chat") {
    active = rid;
    if (mode === "ply" && p.snapshot)
      counter = Math.max(counter, Number(object(p.snapshot).counter) || 0);
    counter++;
    const blocks =
      method === "session/prompt"
        ? array(p.prompt)
        : [{ type: "text", text: p.prompt }, ...array(p.images ?? [])];
    const text = blocks
        .map(object)
        .filter((b) => b.type === "text")
        .map((b) => string(b.text))
        .join("\n"),
      call = `call-${counter}`;
    if (record)
      appendFileSync(
        record,
        `${JSON.stringify({ event: "spawn", fiber: { id: call, plugin: "bash", input: { owned: true } } })}\n`,
      );
    if (mode === "dsh")
      send({
        method: "session/update",
        params: {
          sessionId: session,
          update: {
            sessionUpdate: "tool_call",
            toolCallId: call,
            title: "bash",
            rawInput: { owned: true },
          },
        },
      });
    if (text.includes("delay")) await Bun.sleep(100);
    if (text.includes("approve"))
      await new Promise<void>((resolve, reject) => {
        pending.set(call, { resolve, reject });
        if (mode === "dsh")
          send({
            id: `permit-${call}`,
            method: "session/request_permission",
            params: {
              sessionId: session,
              toolCall: { toolCallId: call },
              options: [
                { kind: "allow_once", optionId: "allow" },
                { kind: "reject_once", optionId: "deny" },
              ],
            },
          });
      });
    if (text.includes("wait"))
      await new Promise<void>((resolve, reject) => pending.set("waiting", { resolve, reject }));
    const output = `native-${counter}:${text}:${blocks.some((v) => object(v).type === "image") ? "pixels" : "text"}`;
    if (record)
      appendFileSync(
        record,
        `${JSON.stringify({ event: "exit", id: call, exit: { status: "done", output: { owned: true } } })}\n`,
      );
    if (mode === "dsh") {
      send({
        method: "session/update",
        params: {
          sessionId: session,
          update: {
            sessionUpdate: "tool_call_update",
            toolCallId: call,
            status: "completed",
            content: [{ type: "content", content: { type: "text", text: "owned result" } }],
          },
        },
      });
      send({
        method: "session/update",
        params: {
          sessionId: session,
          update: {
            sessionUpdate: "agent_message_chunk",
            messageId: `message-${counter}`,
            content: { type: "text", text: output },
          },
        },
      });
      return { stopReason: "end_turn" };
    }
    return { result: { status: "done", text: output }, snapshot: { counter } };
  }
  return {};
}
for await (const line of lines(Bun.stdin.stream(), 40 * 1024 * 1024)) {
  const value = object(parse(line));
  const method = value.method;
  if (method === "session/cancel" || method === "notifications/cancelled") {
    for (const p of pending.values()) p.reject();
    pending.clear();
    continue;
  }
  if (method === undefined && value.id !== undefined) {
    for (const p of pending.values()) p.resolve();
    pending.clear();
    continue;
  }
  if (value.id !== undefined && (typeof value.id === "string" || typeof value.id === "number")) {
    const rid = value.id;
    void request(string(method), object(value.params ?? {}), rid)
      .then((result) => send({ id: rid, result }))
      .catch(() => send({ id: rid, error: { code: -32800, message: "cancelled" } }));
  }
}
void active;
