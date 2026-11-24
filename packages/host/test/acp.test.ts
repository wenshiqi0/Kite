import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { array, type ObjectValue, object, optionalString, parse } from "@kite/protocol";
import { ACP_MESSAGE_LIMIT, ACP_PLACEMENT_METHOD } from "@kite/protocol/acp";
import { lines } from "@kite/runtime";
import { serveAcp } from "@kite/runtime/acp";
import { acpTask, acpToken } from "../src/acp";

const directories: string[] = [],
  servers: ReturnType<typeof serveAcp>[] = [];
afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true);
  for (const dir of directories.splice(0)) rmSync(dir, { recursive: true, force: true });
});
function fixture(hold = false, placement?: { score: number | null; refuseSpawn?: boolean }) {
  const root = mkdtempSync(join(tmpdir(), "kite-acp-client-"));
  directories.push(root);
  const tokenFile = join(root, "token"),
    token = acpToken(tokenFile, true),
    calls: string[] = [],
    created: ObjectValue[] = [],
    permissions: boolean[] = [];
  const server = serveAcp({
    port: 0,
    token,
    connect(send) {
      let promptId: string | number | null | undefined;
      return {
        async receive(message) {
          if (message.method) {
            calls.push(message.method);
            if (message.method === "session/cancel") {
              if (promptId !== undefined)
                send({ id: promptId, result: { stopReason: "cancelled" } });
              return;
            }
            const p = object(message.params ?? {});
            if (message.method === "session/new") created.push(p);
            if (message.method === ACP_PLACEMENT_METHOD && message.id !== undefined) {
              send({ id: message.id, result: { score: placement?.score ?? null } });
              return;
            }
            if (
              message.method === "session/new" &&
              placement?.refuseSpawn &&
              message.id !== undefined
            ) {
              send({ id: message.id, error: { code: -32000, message: "workspace is occupied" } });
              return;
            }
            if (message.method === "session/prompt") {
              send({
                method: "session/update",
                params: {
                  sessionId: "remote-session",
                  update: {
                    sessionUpdate: "tool_call",
                    toolCallId: "compile",
                    status: "in_progress",
                    content: [],
                  },
                },
              });
              send({
                id: "permission",
                method: "session/request_permission",
                params: {
                  sessionId: "remote-session",
                  toolCall: { rawInput: { command: "bun run typecheck", cwd: root } },
                  options: [
                    { optionId: "allow", kind: "allow_once" },
                    { optionId: "deny", kind: "reject_once" },
                  ],
                },
              });
              promptId = message.id;
              return;
            }
            if (message.id !== undefined)
              send({
                id: message.id,
                result:
                  message.method === "initialize"
                    ? {
                        protocolVersion: 1,
                        _meta: {
                          kite: {
                            controlPlaneVersion: 2,
                            controlMethods: placement ? [ACP_PLACEMENT_METHOD] : [],
                          },
                        },
                      }
                    : message.method === "session/prompt"
                      ? { stopReason: "end_turn" }
                      : { sessionId: optionalString(p.sessionId) || "remote-session" },
              });
          } else if (message.id === "permission") {
            permissions.push(object(object(message.result).outcome).optionId === "allow");
            send({
              method: "session/update",
              params: {
                sessionId: "remote-session",
                update: {
                  sessionUpdate: "agent_message_chunk",
                  content: { type: "text", text: "Mini typecheck completed" },
                },
              },
            });
            if (promptId !== undefined && !hold)
              send({ id: promptId, result: { stopReason: "end_turn" } });
          }
        },
        close() {},
      };
    },
  });
  servers.push(server);
  const peer = { url: `ws://127.0.0.1:${server.port}/acp`, tokenFile };
  const peers = join(root, "peers.json");
  writeFileSync(peers, JSON.stringify({ peers: { mini: peer } }), { mode: 0o600 });
  return { root, peer, peers, calls, created, permissions };
}

function callerFixture(root: string) {
  const codexThread = "cb495004-826c-4603-b99a-b2fe50ecd267";
  const managedThread = "cced3f7b-e252-4bb4-b641-3f57f8a7ccd2";
  const codex = { machine: "caller-host", sessionId: "a".repeat(32), provider: "codex" };
  const managed = { machine: codex.machine, sessionId: "b".repeat(32), provider: "antigravity" };
  const catalog = join(root, ".local/share/kite");
  mkdirSync(catalog, { recursive: true });
  writeFileSync(
    join(catalog, "sessions.json"),
    JSON.stringify({
      version: 1,
      revision: 1,
      sessions: [
        {
          id: codex.sessionId,
          title: "Owned Codex fixture",
          provider: codex.provider,
          reference: {
            source: "herdr:codex",
            agent: codex.provider,
            kind: "id",
            value: codexThread,
          },
        },
        {
          id: managed.sessionId,
          title: "Owned managed fixture",
          provider: managed.provider,
          delegation: { parent: codex, policy: "strict" },
          reference: {
            source: "kite:harness",
            agent: managed.provider,
            kind: "id",
            value: managedThread,
          },
        },
      ],
    }),
    { mode: 0o600 },
  );
  const env: NodeJS.ProcessEnv = { ...process.env, HOME: root, KITE_ACP_MACHINE: codex.machine };
  delete env.CODEX_THREAD_ID;
  delete env.KITE_SESSION_ID;
  return { env, codexThread, codex, managed };
}

test("ACP spawn binds the verified caller and requires explicit standalone intent", async () => {
  const f = fixture();
  const caller = callerFixture(f.root);
  const spawn = async (extra: NodeJS.ProcessEnv, ...args: string[]) => {
    const child = Bun.spawn(
      [
        process.execPath,
        resolve("packages/host/src/cli.ts"),
        "acp",
        "spawn",
        "--peers",
        f.peers,
        "--peer",
        "mini",
        "--cwd",
        f.root,
        "--provider",
        "codex",
        ...args,
      ],
      { env: { ...caller.env, ...extra }, stdin: "ignore", stdout: "ignore", stderr: "pipe" },
    );
    const [code, error] = await Promise.all([child.exited, new Response(child.stderr).text()]);
    return { code, error };
  };
  const latest = () => object(object(object(f.created.at(-1)?._meta).kite).delegation);
  const codex = { CODEX_THREAD_ID: caller.codexThread };
  expect(await spawn(codex)).toMatchObject({ code: 0 });
  expect(latest().parent).toEqual(caller.codex);
  expect(await spawn({ ...codex, KITE_SESSION_ID: caller.managed.sessionId })).toMatchObject({
    code: 0,
  });
  expect(latest().parent).toEqual(caller.managed);
  expect(
    (
      await spawn(
        codex,
        "--parent-machine",
        "wrong-host",
        "--parent-session",
        caller.codex.sessionId,
      )
    ).error,
  ).toContain("Explicit parent does not match");
  expect((await spawn({ ...codex, KITE_SESSION_ID: "c".repeat(32) })).error).toContain(
    "Could not verify the active Kite parent",
  );
  expect((await spawn({})).error).toContain("Kite caller context unavailable");
  expect(
    (
      await spawn(
        {},
        "--standalone",
        "--parent-machine",
        "external-host",
        "--parent-session",
        "external-context",
      )
    ).error,
  ).toContain("standalone and parent are mutually exclusive");
  expect(f.created).toHaveLength(2);
  expect(await spawn({}, "--standalone")).toMatchObject({ code: 0 });
  expect(latest().parent).toBeUndefined();
  expect(
    await spawn({}, "--parent-machine", "external-host", "--parent-session", "external-context"),
  ).toMatchObject({ code: 0 });
  expect(latest().parent).toEqual({ machine: "external-host", sessionId: "external-context" });
});

test("ACP defaults to the least loaded eligible peer for a nested child and honors an explicit peer", async () => {
  const busy = fixture(false, { score: 990 }),
    idle = fixture(false, { score: 350 }),
    unmeasured = fixture(false, { score: null }),
    older = fixture();
  const caller = callerFixture(busy.root);
  writeFileSync(
    busy.peers,
    JSON.stringify({
      peers: { busy: busy.peer, idle: idle.peer, unmeasured: unmeasured.peer, older: older.peer },
    }),
  );
  async function spawn(...target: string[]) {
    const child = Bun.spawn(
      [
        process.execPath,
        resolve("packages/host/src/cli.ts"),
        "acp",
        "spawn",
        "--peers",
        busy.peers,
        "--cwd",
        busy.root,
        "--provider",
        "codex",
        ...target,
      ],
      {
        env: { ...caller.env, KITE_SESSION_ID: caller.managed.sessionId },
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(stderr).toBe("");
    expect(code).toBe(0);
    return object(parse(stdout));
  }
  expect((await spawn()).peer).toBe("idle");
  expect(busy.created).toHaveLength(0);
  expect(unmeasured.created).toHaveLength(0);
  expect(older.created).toHaveLength(0);
  const delegated = object(object(idle.created[0]?._meta).kite);
  expect(object(delegated.delegation).parent).toEqual(caller.managed);
  expect((await spawn("--peer", "busy")).peer).toBe("busy");
  expect(busy.created).toHaveLength(1);
  expect(idle.created).toHaveLength(1);
  expect(busy.calls.filter((call) => call === ACP_PLACEMENT_METHOD)).toHaveLength(1);
});

test("a failed spawn on the recommended peer is never retried on another peer", async () => {
  const preferred = fixture(false, { score: 1, refuseSpawn: true }),
    alternate = fixture(false, { score: 2 });
  writeFileSync(
    preferred.peers,
    JSON.stringify({ peers: { preferred: preferred.peer, alternate: alternate.peer } }),
  );
  const invocation =
    await Bun.$`${process.execPath} packages/host/src/cli.ts acp spawn --peers ${preferred.peers} --cwd ${preferred.root} --provider codex --standalone`
      .quiet()
      .nothrow();
  expect(invocation.exitCode).not.toBe(0);
  expect(invocation.stderr.toString()).toContain("ACP spawn on preferred: workspace is occupied");
  expect(preferred.created).toHaveLength(1);
  expect(alternate.created).toHaveLength(0);
});
test("task client returns remote progress and defaults to rejecting unapproved commands", async () => {
  const f = fixture();
  const result = object(await acpTask({ peer: f.peer, cwd: f.root, prompt: "typecheck" }));
  expect(result.sessionId).toBe("remote-session");
  expect(result.text).toBe("Mini typecheck completed");
  expect(f.permissions).toEqual([false]);
  expect(f.calls).toEqual(["initialize", "session/new", "session/prompt"]);
});
test("exact command authorization applies once to the explicitly named workspace", async () => {
  const f = fixture();
  await acpTask({
    peer: f.peer,
    cwd: f.root,
    prompt: "typecheck",
    allowCommands: ["bun run typecheck"],
  });
  expect(f.permissions).toEqual([true]);
});
test("the shipped CLI reads prompts from stdin and emits one structured result", async () => {
  const f = fixture();
  const child = Bun.spawn(
    [
      process.execPath,
      resolve("packages/host/src/cli.ts"),
      "acp",
      "task",
      "--peers",
      f.peers,
      "--peer",
      "mini",
      "--cwd",
      f.root,
    ],
    {
      stdin: Buffer.from("typecheck the shared source"),
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, KITE_AGENT_RESOURCE_CONTROL: "0" },
    },
  );
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  expect(code).toBe(0);
  expect(JSON.parse(stdout)).toMatchObject({
    sessionId: "remote-session",
    text: "Mini typecheck completed",
  });
  expect(stderr).toContain("session/update");
  expect(stdout).not.toContain(acpToken(f.peer.tokenFile));
});

test("MCP processes cancellation while a remote task is in flight", async () => {
  const f = fixture(true),
    child = Bun.spawn(
      [process.execPath, resolve("packages/host/src/cli.ts"), "acp", "mcp", "--peers", f.peers],
      {
        stdin: "pipe",
        stdout: "pipe",
        stderr: "ignore",
        env: { ...process.env, KITE_AGENT_RESOURCE_CONTROL: "0" },
      },
    ),
    reader = child.stdout.getReader();
  let buffer = "";
  const response = async (id: number) => {
    for (;;) {
      const end = buffer.indexOf("\n");
      if (end >= 0) {
        const line = buffer.slice(0, end);
        buffer = buffer.slice(end + 1);
        const value = JSON.parse(line);
        if (value.id === id) return value;
        continue;
      }
      const chunk = await reader.read();
      if (chunk.done) throw new Error("MCP exited");
      buffer += Buffer.from(chunk.value).toString();
    }
  };
  const send = (value: unknown) => {
    child.stdin.write(`${JSON.stringify(value)}\n`);
    void child.stdin.flush();
  };
  try {
    send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: { protocolVersion: "2024-11-05" },
    });
    expect((await response(1)).result.serverInfo.name).toBe("kite-acp");
    send({
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: {
        name: "kite_remote_task",
        arguments: { peer: "mini", cwd: f.root, prompt: "typecheck" },
      },
    });
    for (let n = 0; n < 100 && !f.calls.includes("session/prompt"); n++) await Bun.sleep(5);
    expect(f.calls).toContain("session/prompt");
    send({ jsonrpc: "2.0", method: "notifications/cancelled", params: { requestId: 3 } });
    expect((await response(3)).result.structuredContent.result.stopReason).toBe("cancelled");
    expect(f.calls.filter((method) => method === "session/prompt").length).toBe(1);
  } finally {
    child.stdin.end();
    child.kill();
    reader.releaseLock();
    await child.exited;
  }
});

test("MCP keeps status and cancel available during multi-peer waits and closes losing transports", async () => {
  const dir = mkdtempSync(join(tmpdir(), "kite-mcp-v2-"));
  directories.push(dir);
  const tokenFile = join(dir, "token"),
    token = acpToken(tokenFile, true);
  const id = "b".repeat(32);
  const caller = callerFixture(dir);
  const created: ObjectValue[] = [];
  const waits: (() => void)[] = [];
  let waiting = 0,
    closed = 0;
  const serve = (winner: boolean) => {
    const server = serveAcp({
      port: 0,
      token,
      connect(send) {
        let waitingHere = false;
        return {
          async receive(message) {
            const requestId = message.id;
            if (requestId === undefined) return;
            if (message.method === "initialize")
              send({
                id: requestId,
                result: {
                  protocolVersion: 1,
                  _meta: {
                    kite: { controlPlaneVersion: 2, controlMethods: [ACP_PLACEMENT_METHOD] },
                  },
                },
              });
            if (message.method === ACP_PLACEMENT_METHOD)
              send({ id: requestId, result: { score: winner ? 20 : 100 } });
            if (message.method === "_kite/session/status" || message.method === "session/cancel")
              send({ id: requestId, result: { sessionId: id, state: "working" } });
            if (message.method === "session/new") {
              created.push(object(message.params));
              send({ id: requestId, result: { sessionId: id } });
            }
            if (message.method === "_kite/session/wait_any") {
              waiting++;
              waitingHere = true;
              if (winner)
                waits.push(() =>
                  send({
                    id: requestId,
                    result: {
                      timedOut: false,
                      sessions: [{ sessionId: id, state: "waiting_question" }],
                    },
                  }),
                );
            }
          },
          close() {
            if (waitingHere && !winner) closed++;
          },
        };
      },
    });
    servers.push(server);
    return { url: `ws://127.0.0.1:${server.port}/acp`, tokenFile };
  };
  const peers = join(dir, "peers.json");
  writeFileSync(peers, JSON.stringify({ peers: { a: serve(false), b: serve(true) } }), {
    mode: 0o600,
  });
  const child = Bun.spawn(
    [process.execPath, resolve("packages/host/src/cli.ts"), "acp", "mcp", "--peers", peers],
    {
      env: { ...caller.env, KITE_SESSION_ID: caller.managed.sessionId },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "ignore",
    },
  );
  const responses = new Map<number, unknown>();
  const output = (async () => {
    for await (const line of lines(child.stdout, ACP_MESSAGE_LIMIT)) {
      const row = object(parse(line));
      if (typeof row.id === "number") responses.set(row.id, row.result);
    }
  })();
  const until = async (ready: () => boolean) => {
    for (let attempt = 0; attempt < 300 && !ready(); attempt++) await Bun.sleep(5);
    expect(ready()).toBe(true);
  };
  const request = (id: number, method: string, params: unknown) => {
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    void child.stdin.flush();
  };
  const call = (id: number, name: string, args: unknown) =>
    request(id, "tools/call", { name, arguments: args });
  try {
    request(1, "initialize", { protocolVersion: "2024-11-05" });
    await until(() => responses.has(1));
    expect(object(responses.get(1)).instructions).toContain(
      "Use kite_session_* for every new remote task, including short tasks.",
    );
    expect(object(responses.get(1)).instructions).toContain(
      "If a peer lacks session control v2, report that its gateway needs updating.",
    );
    request(2, "tools/list", {});
    await until(() => responses.has(2));
    const tools = array(object(responses.get(2)).tools).map(object);
    const names = tools.map((tool) => tool.name);
    expect(names).toEqual([
      "kite_session_spawn",
      "kite_session_send",
      "kite_session_status",
      "kite_session_read",
      "kite_session_list",
      "kite_session_wait_any",
      "kite_session_cancel",
      "kite_session_release",
      "kite_remote_task",
    ]);
    expect(tools.find((tool) => tool.name === "kite_remote_task")?.description).toContain(
      "Deprecated. Reserved for existing automation.",
    );
    call(10, "kite_session_spawn", { cwd: dir, provider: "codex" });
    await until(() => responses.has(10));
    expect(object(responses.get(10)).isError).toBe(false);
    expect(object(responses.get(10)).structuredContent).toMatchObject({ peer: "b", sessionId: id });
    expect(created[0]).toMatchObject({
      _meta: { kite: { delegation: { parent: caller.managed } } },
    });
    const targets = [
      { peer: "a", sessionId: id },
      { peer: "b", sessionId: id },
    ];
    for (let i = 3; i < 7; i++) call(i, "kite_session_wait_any", { targets, timeoutMs: 5000 });
    await until(() => waiting === 8);
    call(7, "kite_session_wait_any", { targets, timeoutMs: 5000 });
    call(8, "kite_session_status", { peer: "a", sessionId: id });
    call(9, "kite_session_cancel", { peer: "a", sessionId: id });
    await until(() => [7, 8, 9].every((requestId) => responses.has(requestId)));
    expect(object(responses.get(7)).isError).toBe(true);
    expect(object(responses.get(8)).structuredContent).toMatchObject({ state: "working" });
    expect(object(responses.get(9)).structuredContent).toMatchObject({ state: "working" });
    for (const ready of waits) ready();
    await until(() => [3, 4, 5, 6].every((requestId) => responses.has(requestId)) && closed === 4);
    for (let i = 3; i < 7; i++)
      expect(object(responses.get(i)).structuredContent).toMatchObject({
        peer: "b",
        timedOut: false,
      });
  } finally {
    child.stdin.end();
    child.kill();
    await Promise.allSettled([child.exited, output]);
  }
}, 10_000);
