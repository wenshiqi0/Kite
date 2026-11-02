import { createHash, randomBytes } from "node:crypto";
import { createConnection, type Socket } from "node:net";
import { Readable } from "node:stream";
import { check } from "@kite/protocol";
import { lines } from "@kite/runtime";

const LIMIT = 2 * 1024 * 1024;
class Bytes {
  private pending = Buffer.alloc(0);
  private reader: import("node:stream/web").ReadableStreamDefaultReader<Uint8Array>;
  constructor(socket: Socket) {
    this.reader = (Readable.toWeb(socket) as ReadableStream<Uint8Array>).getReader();
  }
  async exact(count: number): Promise<Buffer> {
    check(count >= 0 && count <= LIMIT, "invalid frame size");
    const parts: Buffer[] = [];
    let remaining = count;
    while (remaining) {
      if (!this.pending.length) {
        const next = await this.reader.read();
        check(!next.done, "backend disconnected");
        this.pending = Buffer.from(next.value);
      }
      const part = this.pending.subarray(0, remaining);
      parts.push(part);
      remaining -= part.length;
      this.pending = this.pending.subarray(part.length);
    }
    return Buffer.concat(parts, count);
  }
}
async function write(socket: Socket, data: Uint8Array): Promise<void> {
  if (!socket.write(data))
    await new Promise<void>((resolve, reject) => {
      socket.once("drain", resolve);
      socket.once("error", reject);
    });
}
export function maskedFrame(opcode: number, data: Buffer): Buffer {
  const mask = randomBytes(4),
    prefix = Buffer.alloc(data.length < 126 ? 2 : data.length < 65536 ? 4 : 10);
  prefix[0] = 0x80 | opcode;
  if (data.length < 126) prefix[1] = 0x80 | data.length;
  else if (data.length < 65536) {
    prefix[1] = 0xfe;
    prefix.writeUInt16BE(data.length, 2);
  } else {
    prefix[1] = 0xff;
    prefix.writeBigUInt64BE(BigInt(data.length), 2);
  }
  const payload = Buffer.from(data);
  for (let i = 0; i < payload.length; i++) payload[i] = (payload[i] ?? 0) ^ (mask[i % 4] ?? 0);
  return Buffer.concat([prefix, mask, payload]);
}
export async function websocketProxy(
  path: string,
  source: ReadableStream<Uint8Array> = Bun.stdin.stream(),
  output: (data: Buffer) => void = (data) => {
    process.stdout.write(data);
  },
): Promise<void> {
  const socket = createConnection(path);
  socket.setTimeout(20000, () => socket.destroy(new Error("backend handshake timed out")));
  try {
    await new Promise<void>((resolve, reject) => {
      socket.once("connect", resolve);
      socket.once("error", reject);
    });
    const bytes = new Bytes(socket),
      key = randomBytes(16).toString("base64");
    await write(
      socket,
      Buffer.from(
        `GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`,
      ),
    );
    let header = "";
    while (!header.endsWith("\r\n\r\n")) {
      check(header.length < 8192, "oversized WebSocket handshake");
      header += (await bytes.exact(1)).toString("latin1");
    }
    const expected = createHash("sha1")
        .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
        .digest("base64"),
      fields = Object.fromEntries(
        header
          .split("\r\n")
          .slice(1)
          .filter((s) => s.includes(":"))
          .map((s) => {
            const at = s.indexOf(":");
            return [s.slice(0, at).toLowerCase(), s.slice(at + 1).trim()];
          }),
      );
    check(
      header.startsWith("HTTP/1.1 101 ") && fields["sec-websocket-accept"] === expected,
      "WebSocket handshake rejected",
    );
    socket.setTimeout(0);
    const input = (async () => {
      for await (const line of lines(source, LIMIT))
        await write(socket, maskedFrame(1, Buffer.from(line)));
      await write(socket, maskedFrame(8, Buffer.alloc(0)));
    })();
    const receive = (async () => {
      let fragments: Buffer[] = [],
        total = 0,
        continuing = false;
      while (true) {
        const h = await bytes.exact(2),
          a = h[0] ?? 0,
          b = h[1] ?? 0,
          opcode = a & 15,
          final = Boolean(a & 128);
        check(!(a & 0x70) && !(b & 128), "invalid server WebSocket frame");
        let size = b & 127;
        if (size === 126) size = (await bytes.exact(2)).readUInt16BE();
        else if (size === 127) {
          const big = (await bytes.exact(8)).readBigUInt64BE();
          check(big <= BigInt(LIMIT), "oversized backend frame");
          size = Number(big);
        }
        check(size <= LIMIT && total + size <= LIMIT, "oversized backend frame");
        check(opcode < 8 || (final && size <= 125), "invalid control frame");
        const data = await bytes.exact(size);
        if (opcode === 8) {
          await write(socket, maskedFrame(8, data.subarray(0, 125)));
          return;
        }
        if (opcode === 9) {
          await write(socket, maskedFrame(10, data));
          continue;
        }
        if (opcode === 10) continue;
        check([0, 1].includes(opcode) && (opcode === 0) === continuing, "unexpected backend frame");
        fragments.push(data);
        total += data.length;
        continuing = !final;
        if (final) {
          const result = Buffer.concat(fragments);
          check(!result.includes(10), "invalid JSONL frame");
          output(Buffer.concat([result, Buffer.from("\n")]));
          fragments = [];
          total = 0;
        }
      }
    })();
    await Promise.race([input, receive]);
  } finally {
    socket.destroy();
  }
}
