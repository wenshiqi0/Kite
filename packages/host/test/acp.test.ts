import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { array, object, optionalString, parse } from "@kite/protocol";
import { ACP_MESSAGE_LIMIT } from "@kite/protocol/acp";
import { lines } from "@kite/runtime";
import { serveAcp } from "@kite/runtime/acp";
import { acpTask, acpToken } from "../src/acp";

const directories: string[] = [],
  servers: ReturnType<typeof serveAcp>[] = [];
afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true);
  for (const dir of directories.splice(0)) rmSync(dir, { recursive: true, force: true });
});
function fixture(hold = false) {
  const root = mkdtempSync(join(tmpdir(), "kite-acp-client-"));
  directories.push(root);
  const tokenFile = join(root, "token"),
    token = acpToken(tokenFile, true),
    calls: string[] = [],
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
                    ? { protocolVersion: 1 }
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
  return { root, peer, peers, calls, permissions };
}
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
                result: { protocolVersion: 1, _meta: { kite: { controlPlaneVersion: 2 } } },
              });
            if (message.method === "_kite/session/status" || message.method === "session/cancel")
              send({ id: requestId, result: { sessionId: id, state: "working" } });
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
    { stdin: "pipe", stdout: "pipe", stderr: "ignore" },
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
    request(2, "tools/list", {});
    await until(() => responses.has(2));
    const names = array(object(responses.get(2)).tools).map((tool) => object(tool).name);
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
