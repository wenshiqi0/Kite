import { afterEach, expect, test } from "bun:test";
import type { RpcMessage } from "@kite/protocol";
import { connectAcp, serveAcp } from "../src/acp";

const token = "a".repeat(64),
  servers: ReturnType<typeof serveAcp>[] = [];
afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true);
});
function start(connected: () => void = () => {}) {
  const server = serveAcp({
    token,
    port: 0,
    connect(send) {
      connected();
      return {
        async receive(message) {
          if (message.id !== undefined)
            send({
              id: message.id,
              result: { method: message.method || "", params: message.params ?? {} },
            });
        },
        close() {},
      };
    },
  });
  servers.push(server);
  return `ws://127.0.0.1:${server.port}/acp`;
}
test("WebSocket ACP authenticates before constructing an agent and rejects browser origins", async () => {
  let created = 0;
  const url = start(() => created++);
  await expect(connectAcp(url, "b".repeat(64), () => {})).rejects.toThrow("handshake failed");
  const http = url.replace("ws:", "http:");
  expect(
    (
      await fetch(http, {
        headers: { Authorization: `Bearer ${token}`, Origin: "https://example.invalid" },
      })
    ).status,
  ).toBe(403);
  expect(
    (await fetch(http, { method: "POST", headers: { Authorization: `Bearer ${token}` } })).status,
  ).toBe(405);
  expect(created).toBe(0);
});
test("ACP preserves UTF-8 JSON-RPC IDs and complete messages over the actual socket", async () => {
  const url = start(),
    received: RpcMessage[] = [],
    done = Promise.withResolvers<void>();
  const client = await connectAcp(url, token, (message) => {
    received.push(message);
    done.resolve();
  });
  try {
    client.send({
      id: "request-棋",
      method: "session/new",
      params: { cwd: "/项目/mini", mcpServers: [] },
    });
    await done.promise;
    expect(received).toEqual([
      {
        jsonrpc: "2.0",
        id: "request-棋",
        result: { method: "session/new", params: { cwd: "/项目/mini", mcpServers: [] } },
      },
    ]);
  } finally {
    client.close();
  }
});
test("ACP refuses plaintext non-loopback endpoints and invalid envelopes", async () => {
  await expect(connectAcp("ws://192.0.2.1:9000/acp", token, () => {})).rejects.toThrow(
    "loopback SSH forward",
  );
  const url = start();
  const socket = new WebSocket(url, { headers: { Authorization: `Bearer ${token}` } });
  await new Promise<void>((resolve, reject) => {
    socket.onopen = () => resolve();
    socket.onerror = () => reject(new Error("open failed"));
  });
  const closed = new Promise<number>((resolve) => {
    socket.onclose = (event) => resolve(event.code);
  });
  socket.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", result: {} }));
  expect(await closed).toBe(1008);
});
