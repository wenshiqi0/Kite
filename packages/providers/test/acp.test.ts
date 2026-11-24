import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  array,
  type Json,
  object,
  type RpcMessage,
  type SessionRecord,
  string,
} from "@kite/protocol";
import { ACP_PLACEMENT_METHOD } from "@kite/protocol/acp";
import { type AcpBackend, type AcpRepository, KiteAcp, kiteAcpConnector } from "../src/acp";

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
const antigravitySettings = [
  {
    id: "tool_permission",
    currentValue: "turbo",
    options: [{ value: "default" }, { value: "turbo" }],
  },
];
function fixture(placement?: () => Promise<number | null>) {
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
      stateDirectory: root,
      defaultProvider: "ply",
      repository,
      ...(placement ? { placement } : {}),
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
          ...(record.provider === "antigravity" ? { settings: antigravitySettings } : {}),
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

test("resource placement checks cwd and provider without starting an owner", async () => {
  let samples = 0;
  const f = fixture(async () => ++samples);
  const outside = directory();
  symlinkSync(outside, join(f.root, "escape"));
  await f.initialize();
  const capabilities = object(object(object(f.outputs[0]?.result)._meta).kite);
  expect(array(capabilities.controlMethods)).toContain(ACP_PLACEMENT_METHOD);
  const probe = (cwd: string, provider: string) =>
    f.agent.receive({ id: 2, method: ACP_PLACEMENT_METHOD, params: { cwd, provider } });
  await probe(f.root, "codex");
  expect(f.outputs.at(-1)?.result).toEqual({ score: 1 });
  await probe(f.root, "claude");
  expect(f.outputs.at(-1)?.error).toBeDefined();
  await probe(join(f.root, "escape"), "codex");
  expect(f.outputs.at(-1)?.error).toBeDefined();
  expect(samples).toBe(1);
  expect(f.created()).toBe(0);
  expect(f.owners.size).toBe(0);
});
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

test("legacy ACP prompts retain the strict approval path with an Antigravity Turbo owner", async () => {
  const f = fixture();
  const record = f.records[0];
  if (!record) throw new Error("missing fixture record");
  record.provider = "antigravity";
  await f.initialize();
  await f.resume(record.id);
  const pending = f.agent.receive({
    id: 9,
    method: "session/prompt",
    params: { sessionId: record.id, prompt: [{ type: "text", text: "owned" }] },
  });
  const owner = f.owners.get(record.id);
  await until(() => owner?.active === true);
  expect(owner?.calls.find(([method]) => method === "turn/start")?.[1]).toMatchObject({
    sessionSettings: { tool_permission: "default" },
  });
  f.agent.close();
  await pending;
});

function controlFixture() {
  const root = directory();
  const records: SessionRecord[] = [];
  const runs = new Map<string, { turn: string; params: Json; send(message: RpcMessage): void }>();
  const endings = new Map<string, string>();
  const fixture = kiteAcpConnector({
    machine: "owned-fixture",
    herdr: "unused",
    workspaces: [root],
    defaultProvider: "codex",
    stateDirectory: root,
    humanAvailable: async () => true,
    repository: {
      list: async () => records,
      async create(provider, cwd, title, delegation, selection) {
        if (provider !== "codex" && provider !== "ply" && provider !== "antigravity")
          throw new Error("unexpected provider");
        const row: SessionRecord = {
          id: crypto.randomUUID().replaceAll("-", ""),
          provider,
          cwd,
          title,
          ...(delegation ? { delegation } : {}),
          ...(selection ? { acp_selection: selection } : {}),
        };
        records.push(row);
        return row;
      },
    },
    inspect: async (record) => {
      if (record.archived) return { state: "released" };
      const run = runs.get(record.id);
      const status = endings.get(record.id);
      return run
        ? { state: "working", activeTurnId: run.turn }
        : { state: "idle", ...(status ? { lastTurnStatus: status } : {}) };
    },
    release: async (record) => {
      record.archived = true;
    },
    async backend(record, event) {
      const thread = `thread-${record.id}`;
      return {
        thread,
        model: "fixture",
        effort: "high",
        ...(record.provider === "ply"
          ? {
              settings: [
                {
                  id: "approval_mode",
                  currentValue: "bypass",
                  options: [{ value: "bypass" }, { value: "ask" }],
                },
              ],
            }
          : {}),
        ...(record.provider === "antigravity" ? { settings: antigravitySettings } : {}),
        models: [
          {
            model: "fixture",
            defaultReasoningEffort: "high",
            supportedReasoningEfforts: [{ reasoningEffort: "high" }, { reasoningEffort: "low" }],
            serviceTiers: [{ id: "priority" }],
          },
        ],
        async call(method, params) {
          if (method === "turn/start") {
            const turn = crypto.randomUUID();
            const send = (message: RpcMessage) =>
              event({
                ...message,
                params: {
                  threadId: thread,
                  turnId: turn,
                  ...object(message.params ?? {}),
                },
              });
            runs.set(record.id, { turn, params, send });
            return { turn: { id: turn } };
          }
          if (method === "turn/interrupt") finish(record.id, "interrupted");
          return {};
        },
        write() {},
        close() {},
      };
    },
  });
  function finish(id: string, status = "completed") {
    const binding = runs.get(id);
    if (!binding) throw new Error("owner unavailable");
    runs.delete(id);
    endings.set(id, status);
    binding.send({ method: "turn/completed", params: { turn: { id: binding.turn, status } } });
  }
  async function client() {
    let sequence = 1;
    const outputs = new Map<string | number, RpcMessage>();
    const agent = fixture((message) => {
      if (typeof message.id === "number") outputs.set(message.id, message);
    });
    async function call(method: string, params: Json = {}) {
      const id = sequence++;
      await agent.receive({ id, method, params });
      const response = outputs.get(id);
      if (response?.error) throw new Error(response.error.message);
      return object(response?.result);
    }
    await call("initialize", { protocolVersion: 1 });
    return { call, close: () => agent.close() };
  }
  const prompt = [{ type: "text", text: "owned turn" }];
  return { root, records, runs, finish, client, prompt };
}

test("v2 pins the manual approval path for strict and human policies on Antigravity", async () => {
  const f = controlFixture(),
    client = await f.client();
  const child = await client.call("session/new", {
    cwd: f.root,
    mcpServers: [],
    _meta: { kite: { provider: "antigravity", controlPlaneVersion: 2 } },
  });
  const sessionId = string(child.sessionId);
  for (const policy of ["strict", "human_in_the_loop"]) {
    await client.call("_kite/session/submit", { sessionId, prompt: f.prompt, policy });
    expect(f.runs.get(sessionId)?.params).toMatchObject({
      sessionSettings: { tool_permission: "default" },
    });
    f.finish(sessionId);
  }
  client.close();
});

test("v2 validates model and service tier, exposes question transitions and bounds encoded readback", async () => {
  const f = controlFixture(),
    author = await f.client(),
    observer = await f.client();
  const child = await author.call("session/new", {
    cwd: f.root,
    mcpServers: [],
    _meta: {
      kite: {
        provider: "codex",
        model: "fixture",
        effort: "low",
        serviceTier: "priority",
        delegation: { policy: "human_in_the_loop" },
      },
    },
  });
  const id = string(child.sessionId);
  const send = (fields: Record<string, Json> = {}) =>
    author.call("_kite/session/submit", { sessionId: id, prompt: f.prompt, ...fields });
  await expect(send({ serviceTier: "bad" })).rejects.toThrow("service tier");
  await expect(send({ model: "unknown" })).rejects.toThrow("model unavailable");
  expect(f.runs.size).toBe(0);
  const turn = await send({ serviceTier: "fast", effort: "high" });
  const working = await observer.call("_kite/session/status", { sessionId: id });
  expect(working).toMatchObject({
    state: "working",
    activeTurnId: turn.turnId,
    model: "fixture",
    effort: "high",
    serviceTier: "priority",
  });
  const run = f.runs.get(id);
  expect(run?.params).toMatchObject({ model: "fixture", effort: "high", serviceTier: "priority" });
  author.close();
  expect(
    (await observer.call("_kite/session/wait_any", { sessionIds: [id], timeoutMs: 5 })).timedOut,
  ).toBe(true);
  expect(f.runs.size).toBe(1);
  const waiting = observer.call("_kite/session/wait_any", { sessionIds: [id], timeoutMs: 5000 });
  run?.send({
    id: "native-question",
    method: "item/tool/requestUserInput",
    params: {
      questions: [{ id: "path", question: "Which workspace?" }],
    },
  });
  const blocked = await waiting;
  expect(blocked).toMatchObject({ timedOut: false, sessionId: id });
  expect(array(blocked.sessions)[0]).toMatchObject({
    state: "waiting_question",
    pendingInteraction: { id: "native-question", question: "Which workspace?" },
  });
  run?.send({ method: "serverRequest/resolved", params: { requestId: "native-question" } });
  expect((await observer.call("_kite/session/status", { sessionId: id })).state).toBe("working");
  run?.send({
    method: "item/completed",
    params: {
      item: { id: "reply", type: "agentMessage", text: `${"\n`华😀".repeat(100_000)} final tail` },
    },
  });
  run?.send({
    method: "item/completed",
    params: {
      item: {
        id: "tool",
        type: "commandExecution",
        command: "bun run check",
        exitCode: 2,
        aggregatedOutput: `${"start\n".repeat(100_000)}tool tail`,
      },
    },
  });
  f.finish(id);
  const read = (maxBytes?: number) =>
    observer.call("_kite/session/read", { sessionId: id, turnId: turn.turnId, maxBytes });
  await expect(read(262145)).rejects.toThrow("limit");
  for (const size of [1024, 65536, 262144]) {
    const result = await read(size);
    expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(size);
    expect(result.truncated).toBe(true);
    expect(string(result.reply).endsWith("final tail")).toBe(true);
    expect(string(result.reply)).not.toContain("\ufffd");
    expect(array(result.tools)[0]).toMatchObject({ exitCode: 2, status: "failed" });
  }
  expect((await observer.call("_kite/session/release", { sessionId: id })).state).toBe("released");
  expect((await read(1024)).truncated).toBe(true);
  observer.close();
});

test("v2 rejects path traversal and attachment escape, permits separate linked worktrees and forces release explicitly", async () => {
  const f = controlFixture(),
    client = await f.client();
  const repo = join(f.root, "repository"),
    first = join(f.root, "first"),
    second = join(f.root, "second");
  const git = async (...args: string[]) => {
    const child = Bun.spawn(["git", "-C", repo, ...args], {
      stdin: "ignore",
      stdout: "ignore",
      stderr: "ignore",
    });
    expect(await child.exited).toBe(0);
  };
  mkdirSync(repo);
  await git("init");
  await git(
    "-c",
    "user.name=Fixture",
    "-c",
    "user.email=fixture@example.invalid",
    "commit",
    "--allow-empty",
    "-m",
    "owned",
  );
  await git("worktree", "add", "-b", "first", first);
  await git("worktree", "add", "-b", "second", second);
  const spawn = (cwd: string, provider = "codex", policy = "strict") =>
    client.call("session/new", {
      cwd,
      mcpServers: [],
      _meta: {
        kite: { provider, delegation: { worktreeIsolated: true, writeLock: true, policy } },
      },
    });
  await expect(spawn(repo)).rejects.toThrow("linked Git worktree");
  await expect(spawn(first, "codex", "isolated_auto")).rejects.toThrow("not enabled");
  const one = string((await spawn(first)).sessionId);
  const two = string((await spawn(second, "ply", "isolated_auto")).sessionId);
  symlinkSync(join(directory(), "missing"), join(first, "escape"));
  const submit = (id: string, blocks: Json = f.prompt) =>
    client.call("_kite/session/submit", { sessionId: id, prompt: blocks });
  for (const uri of [
    `file://${first}/escape`,
    `file://${repo}/../first/some.txt`,
    "https://example.invalid/a",
  ]) {
    await expect(submit(one, [{ type: "resource_link", uri }])).rejects.toThrow();
  }
  const allowed = join(first, "owned file.txt");
  writeFileSync(allowed, "owned attachment");
  await submit(one, [{ type: "resource_link", uri: allowed }]);
  expect(f.runs.get(one)?.params).toMatchObject({
    input: [{ type: "text", text: `file://${first}/owned%20file.txt` }],
  });
  await submit(two);
  expect(f.runs.get(two)?.params).toMatchObject({ sessionSettings: { approval_mode: "bypass" } });
  await expect(spawn(first)).rejects.toThrow("workspace");
  expect((await client.call("_kite/session/release", { sessionId: one })).released).toBe(false);
  expect(
    (await client.call("_kite/session/release", { sessionId: one, forceCancel: true })).state,
  ).toBe("released");
  expect(f.runs.has(two)).toBe(true);
  f.finish(two);
  client.close();
});

test("short waits return the completed reply and release is idempotent", async () => {
  const f = controlFixture(),
    client = await f.client();
  const id = string(
    (
      await client.call("session/new", {
        cwd: f.root,
        mcpServers: [],
        _meta: { kite: { controlPlaneVersion: 2 } },
      })
    ).sessionId,
  );
  const result = client.call("_kite/session/submit", {
    sessionId: id,
    prompt: f.prompt,
    waitMs: 1000,
  });
  await until(() => f.runs.has(id));
  f.runs.get(id)?.send({
    method: "item/completed",
    params: {
      item: { id: "recap", type: "agentMessage", text: "owned reply" },
    },
  });
  f.finish(id);
  expect(await result).toMatchObject({
    completed: true,
    reply: "owned reply",
    stopReason: "end_turn",
  });
  for (let i = 0; i < 2; i++) {
    expect(await client.call("_kite/session/release", { sessionId: id })).toMatchObject({
      state: "released",
      released: true,
    });
  }
  client.close();
});
