import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Json, object, type RpcMessage, type SessionRecord } from "@kite/protocol";
import { type AcpBackend, type AcpRepository, KiteAcp } from "../src/acp";

const directories: string[] = [];
afterEach(() => {
  for (const dir of directories.splice(0)) rmSync(dir, { recursive: true, force: true });
});
function directory() {
  const dir = mkdtempSync(join(tmpdir(), "kite-network-acp-"));
  directories.push(dir);
  return dir;
}
const until = async (check: () => boolean) => {
  for (let n = 0; n < 200; n++) {
    if (check()) return;
    await Bun.sleep(5);
  }
  throw new Error("ACP condition timed out");
};
function fixture() {
  const root = directory(),
    records: SessionRecord[] = [
      { id: "a".repeat(32), provider: "ply", cwd: root, title: "mini" },
      { id: "b".repeat(32), provider: "ply", cwd: root, title: "local" },
    ],
    outputs: RpcMessage[] = [],
    owners = new Map<
      string,
      {
        event: (message: RpcMessage) => void;
        request: (message: RpcMessage) => void;
        calls: [string, Json][];
        writes: RpcMessage[];
        active: boolean;
        closed: boolean;
      }
    >();
  let created = 0;
  const repository: AcpRepository = {
    list: async () => records,
    async create(provider, cwd, title) {
      created++;
      if (provider !== "codex" && provider !== "ply" && provider !== "dsh")
        throw new Error("unexpected fixture provider");
      const record: SessionRecord = {
        id: "c".repeat(32),
        provider,
        cwd,
        title,
      };
      records.push(record);
      return record;
    },
  };
  const agent = new KiteAcp(
    {
      machine: "mini",
      herdr: "/owned/herdr",
      workspaces: [root],
      defaultProvider: "ply",
      repository,
      async backend(record, event, request): Promise<AcpBackend> {
        const owner = {
          event,
          request,
          calls: [] as [string, Json][],
          writes: [] as RpcMessage[],
          active: false,
          closed: false,
        };
        owners.set(record.id, owner);
        return {
          thread: `thread-${record.id}`,
          model: "model",
          effort: "",
          async call(method, params) {
            owner.calls.push([method, params]);
            if (method === "thread/read") {
              return {
                thread: {
                  id: `thread-${record.id}`,
                  status: { type: owner.active ? "active" : "idle" },
                },
              };
            }
            if (method === "thread/turns/list") {
              if (record.provider === "codex") throw new Error("thread is not materialized yet");
              return { data: owner.active ? [{ id: "turn", status: "inProgress" }] : [] };
            }
            if (method === "turn/start") {
              owner.active = true;
              return { turn: { id: "turn" } };
            }
            if (method === "turn/interrupt") {
              owner.active = false;
              owner.event({
                method: "turn/completed",
                params: {
                  threadId: `thread-${record.id}`,
                  turn: { id: "turn", status: "interrupted" },
                },
              });
            }
            return {};
          },
          write(message) {
            owner.writes.push(message);
          },
          close() {
            owner.closed = true;
          },
        };
      },
    },
    (message) => outputs.push(message),
  );
  const initialize = () =>
      agent.receive({ id: 1, method: "initialize", params: { protocolVersion: 1 } }),
    resume = (id: string) =>
      agent.receive({
        id: 2,
        method: "session/resume",
        params: { sessionId: id, cwd: root, mcpServers: [] },
      });
  return { root, records, outputs, owners, agent, initialize, resume, created: () => created };
}
test("ACP session listing is read-only and rejects workspace and symlink escapes", async () => {
  const f = fixture(),
    outside = directory();
  mkdirSync(join(f.root, "nested"));
  symlinkSync(outside, join(f.root, "escape"));
  await f.initialize();
  expect(object(object(f.outputs[0]?.result).agentCapabilities).loadSession).toBe(false);
  await f.agent.receive({ id: 3, method: "session/list", params: {} });
  expect(f.owners.size).toBe(0);
  for (const cwd of [outside, join(f.root, "escape"), "relative"]) {
    await f.agent.receive({ id: 4, method: "session/new", params: { cwd, mcpServers: [] } });
    expect(f.outputs.at(-1)?.error).toBeDefined();
  }
  expect(f.created()).toBe(0);
  f.agent.close();
});
test("permissions bind the ACP response to the original session even when native IDs collide", async () => {
  const f = fixture();
  await f.initialize();
  for (const record of f.records) await f.resume(record.id);
  for (const record of f.records)
    f.owners.get(record.id)?.request({
      id: "same-native-id",
      method: "item/commandExecution/requestApproval",
      params: {
        threadId: `thread-${record.id}`,
        turnId: "turn",
        command: "bun run typecheck",
        cwd: f.root,
      },
    });
  const requests = f.outputs.filter((message) => message.method === "session/request_permission");
  expect(requests.length).toBe(2);
  expect(requests[0]?.id).not.toBe(requests[1]?.id);
  await f.agent.receive({
    id: requests[0]?.id ?? null,
    result: { outcome: { outcome: "selected", optionId: "allow-once" } },
  });
  expect(f.owners.get(f.records[0]?.id || "")?.writes).toEqual([
    { id: "same-native-id", result: { decision: "accept" } },
  ]);
  expect(f.owners.get(f.records[1]?.id || "")?.writes).toEqual([]);
  await f.agent.receive({
    id: requests[0]?.id ?? null,
    result: { outcome: { outcome: "selected", optionId: "allow-once" } },
  });
  expect(f.owners.get(f.records[0]?.id || "")?.writes.length).toBe(1);
  f.agent.close();
});
test("prompt cancellation reaches its owner while another session stays attached", async () => {
  const f = fixture();
  await f.initialize();
  const id = f.records[0]?.id || "";
  await f.resume(id);
  await f.resume(f.records[1]?.id || "");
  const pending = f.agent.receive({
    id: 9,
    method: "session/prompt",
    params: { sessionId: id, prompt: [{ type: "text", text: "build" }] },
  });
  await until(() => f.owners.get(id)?.active === true);
  await f.agent.receive({ method: "session/cancel", params: { sessionId: id } });
  await pending;
  expect(f.outputs.find((r) => r.id === 9)?.result).toMatchObject({ stopReason: "cancelled" });
  expect(f.owners.get(f.records[1]?.id || "")?.calls).toEqual([]);
  f.agent.close();
});
test("Codex ACP starts the first turn from live thread status and rejects an active thread", async () => {
  const f = fixture();
  await f.initialize();
  await f.agent.receive({
    id: 2,
    method: "session/new",
    params: { cwd: f.root, mcpServers: [], _meta: { kite: { provider: "codex" } } },
  });
  const id = String(object(f.outputs.find((r) => r.id === 2)?.result).sessionId),
    owner = f.owners.get(id);
  if (!owner) throw new Error("Codex owner not attached");
  const prompt = (requestId: number) =>
    f.agent.receive({
      id: requestId,
      method: "session/prompt",
      params: { sessionId: id, prompt: [{ type: "text", text: "inspect" }] },
    });

  owner.active = true;
  await prompt(3);
  expect(f.outputs.find((r) => r.id === 3)?.error?.message).toBe(
    "remote session already has an active turn",
  );
  expect(owner.calls.map(([method]) => method)).toEqual(["thread/read"]);

  owner.active = false;
  const first = prompt(4);
  await until(() => owner.active);
  expect(owner.calls.slice(1).map(([method]) => method)).toEqual(["thread/read", "turn/start"]);
  owner.event({
    method: "turn/completed",
    params: { threadId: `thread-${id}`, turn: { id: "turn", status: "completed" } },
  });
  await first;
  expect(f.outputs.find((r) => r.id === 4)?.result).toMatchObject({ stopReason: "end_turn" });
  f.agent.close();
});
test("streamed assistant deltas are not repeated by the final completed item", async () => {
  const f = fixture();
  await f.initialize();
  const id = f.records[0]?.id || "";
  await f.resume(id);
  const pending = f.agent.receive({
    id: 9,
    method: "session/prompt",
    params: { sessionId: id, prompt: [{ type: "text", text: "build" }] },
  });
  await until(() => f.owners.get(id)?.active === true);
  const owner = f.owners.get(id);
  expect(owner).toBeDefined();
  owner?.event({
    method: "item/agentMessage/delta",
    params: { threadId: `thread-${id}`, itemId: "answer", delta: "passed" },
  });
  owner?.event({
    method: "item/completed",
    params: {
      threadId: `thread-${id}`,
      item: { id: "answer", type: "agentMessage", text: "passed" },
    },
  });
  owner?.event({
    method: "turn/completed",
    params: { threadId: `thread-${id}`, turn: { id: "turn", status: "completed" } },
  });
  await pending;
  const chunks = f.outputs
    .filter((r) => r.method === "session/update")
    .map((r) => object(object(r.params).update));
  expect(
    chunks
      .filter((r) => r.sessionUpdate === "agent_message_chunk")
      .map((r) => object(r.content).text),
  ).toEqual(["passed"]);
  f.agent.close();
});
test("disconnect detaches the transport without cancelling or replaying its remote turn", async () => {
  const f = fixture();
  await f.initialize();
  const id = f.records[0]?.id || "";
  await f.resume(id);
  const pending = f.agent.receive({
    id: 9,
    method: "session/prompt",
    params: { sessionId: id, prompt: [{ type: "text", text: "build" }] },
  });
  await until(() => f.owners.get(id)?.active === true);
  f.agent.close();
  await pending;
  const owner = f.owners.get(id);
  expect(owner?.closed).toBe(true);
  expect(owner?.active).toBe(true);
  expect(owner?.calls.filter(([method]) => method === "turn/start").length).toBe(1);
  expect(owner?.calls.some(([method]) => method === "turn/interrupt")).toBe(false);
});
