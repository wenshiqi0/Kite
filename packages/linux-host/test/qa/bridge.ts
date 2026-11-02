import { createConnection, type Socket } from "node:net";
import { Readable } from "node:stream";
import { check, object, parse } from "@kite/protocol";

export class Bridge {
  private pending = Buffer.alloc(0);
  private reader: import("node:stream/web").ReadableStreamDefaultReader<Uint8Array>;
  private lines = Buffer.alloc(0);
  private constructor(readonly socket: Socket) {
    this.reader = (Readable.toWeb(socket) as ReadableStream<Uint8Array>).getReader();
  }
  static async connect(address: string, herdr: string): Promise<Bridge> {
    const socket = createConnection(address);
    socket.setTimeout(6000, () => socket.destroy(new Error("owned bridge timed out")));
    await new Promise<void>((resolve, reject) => {
      socket.once("connect", resolve);
      socket.once("error", reject);
    });
    const bridge = new Bridge(socket);
    socket.write(
      `${JSON.stringify({ method: "herdr-api", args: ["--herdr", herdr], params: {} })}\n`,
    );
    const line: number[] = [];
    while (!line.length || line.at(-1) !== 10) {
      check(line.length < 65536, "owned bridge ack too large");
      line.push((await bridge.exact(1))[0] || 0);
    }
    check(object(parse(Buffer.from(line).toString())).ok, "owned bridge rejected");
    socket.setTimeout(0);
    return bridge;
  }
  private async exact(count: number): Promise<Buffer> {
    const parts: Buffer[] = [];
    while (count) {
      if (!this.pending.length) {
        const timer = setTimeout(
          () => this.socket.destroy(new Error("owned bridge read timed out")),
          6000,
        );
        try {
          const next = await this.reader.read();
          check(!next.done, "owned bridge disconnected during upgrade");
          this.pending = Buffer.from(next.value);
        } finally {
          clearTimeout(timer);
        }
      }
      const part = this.pending.subarray(0, count);
      parts.push(part);
      count -= part.length;
      this.pending = this.pending.subarray(part.length);
    }
    return Buffer.concat(parts);
  }
  send(data: string, kind = 0): void {
    const payload = Buffer.from(data),
      header = Buffer.alloc(5);
    header[0] = kind;
    header.writeUInt32BE(payload.length, 1);
    this.socket.write(Buffer.concat([header, payload]));
  }
  private async frame(): Promise<{ kind: number; body: Buffer }> {
    const header = await this.exact(5),
      count = header.readUInt32BE(1);
    check(count <= 65536, "owned bridge frame too large");
    return { kind: header[0] || 0, body: await this.exact(count) };
  }
  async line(): Promise<string> {
    while (!this.lines.includes(10)) {
      const frame = await this.frame();
      check(frame.kind === 1, "owned bridge returned a non-output frame");
      this.lines = Buffer.concat([this.lines, frame.body]);
    }
    const at = this.lines.indexOf(10),
      line = this.lines.subarray(0, at).toString();
    this.lines = this.lines.subarray(at + 1);
    return line;
  }
  async close(): Promise<void> {
    this.send("", 4);
    const frame = await this.frame();
    check(
      frame.kind === 3 && frame.body.equals(Buffer.alloc(4)),
      `owned bridge exit was not clean: kind=${frame.kind} body=${frame.body.toString("hex").slice(0, 512)}`,
    );
    this.socket.destroy();
  }
}
