import { afterAll, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { check } from "@kite/protocol";
import { websocketProxy } from "./websocket";

const root = mkdtempSync(join(tmpdir(), "kite-ws-test-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
function frame(opcode: number, body: Buffer, final = true): Buffer {
  const header = Buffer.alloc(body.length < 126 ? 2 : 4);
  header[0] = (final ? 128 : 0) | opcode;
  if (body.length < 126) header[1] = body.length;
  else {
    header[1] = 126;
    header.writeUInt16BE(body.length, 2);
  }
  return Buffer.concat([header, body]);
}
async function peer(
  path: string,
  after: (socket: Socket) => void,
): Promise<ReturnType<typeof createServer>> {
  const server = createServer((socket) => {
    let header = Buffer.alloc(0);
    const handshake = (data: Buffer) => {
      header = Buffer.concat([header, data]);
      if (!header.includes("\r\n\r\n")) return;
      socket.off("data", handshake);
      const key = /Sec-WebSocket-Key: ([^\r]+)/.exec(header.toString())?.[1];
      check(key, "missing client key");
      const accept = createHash("sha1")
        .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
        .digest("base64");
      socket.write(
        `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
      );
      after(socket);
    };
    socket.on("data", handshake);
    socket.on("error", () => {});
  });
  await new Promise<void>((r) => server.listen(path, r));
  return server;
}
test("Unix WebSocket proxy authenticates upgrade and rejoins fragmented JSON", async () => {
  const path = join(root, "valid.sock"),
    source = new ReadableStream<Uint8Array>({ start() {} }),
    rows: Buffer[] = [];
  const server = await peer(path, (s) => {
    s.write(frame(1, Buffer.from('{"result":'), false));
    s.write(frame(9, Buffer.from("ping")));
    s.write(frame(0, Buffer.from('"owned"}')));
    s.write(frame(8, Buffer.alloc(0)));
  });
  try {
    await websocketProxy(path, source, (data) => rows.push(data));
    expect(Buffer.concat(rows).toString()).toBe('{"result":"owned"}\n');
  } finally {
    server.close();
  }
});
test("rejects oversized server frames before allocating their payload", async () => {
  const path = join(root, "oversized.sock"),
    server = await peer(path, (s) => {
      const header = Buffer.alloc(10);
      header[0] = 0x81;
      header[1] = 127;
      header.writeBigUInt64BE(4000000n, 2);
      s.write(header);
    });
  try {
    await expect(
      websocketProxy(path, new ReadableStream<Uint8Array>({ start() {} })),
    ).rejects.toThrow("oversized");
  } finally {
    server.close();
  }
});
test("rejects line breaks inside a JSONL message", async () => {
  const path = join(root, "newline.sock"),
    server = await peer(path, (s) => s.write(frame(1, Buffer.from('{\n"owned":true}'))));
  try {
    await expect(
      websocketProxy(path, new ReadableStream<Uint8Array>({ start() {} })),
    ).rejects.toThrow("JSONL");
  } finally {
    server.close();
  }
});
