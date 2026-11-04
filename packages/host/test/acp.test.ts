import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { object, optionalString } from "@kite/protocol";
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
    child.stdin.write(JSON.stringify(value) + "\n");
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
