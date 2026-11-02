import { createConnection } from "node:net";
import { Readable } from "node:stream";
import { check, type Json, object, parse } from "@kite/protocol";
import { bounded } from "./index";
export async function listening(path: string): Promise<boolean> {
  return new Promise((resolve) => {
    const s = createConnection(path);
    const timer = setTimeout(() => {
      s.destroy();
      resolve(false);
    }, 500);
    s.once("connect", () => {
      clearTimeout(timer);
      s.destroy();
      resolve(true);
    });
    s.once("error", () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}
export async function socketRequest(path: string, value: Json, limit = 4096): Promise<Json> {
  const socket = createConnection(path);
  const timer = setTimeout(() => socket.destroy(new Error("socket request timed out")), 5000);
  try {
    await new Promise<void>((resolve, reject) => {
      socket.once("connect", resolve);
      socket.once("error", reject);
    });
    socket.write(`${JSON.stringify(value)}\n`);
    let buffer = Buffer.alloc(0);
    for await (const chunk of Readable.toWeb(socket) as ReadableStream<Uint8Array>) {
      buffer = Buffer.concat([buffer, chunk]);
      check(buffer.length <= limit, "socket response exceeds limit");
      const end = buffer.indexOf(10);
      if (end >= 0) return parse(buffer.subarray(0, end).toString());
    }
    throw new Error("socket disconnected");
  } finally {
    clearTimeout(timer);
    socket.destroy();
  }
}
export async function bridgeSocket(path: string): Promise<void> {
  const socket = createConnection(path);
  await new Promise<void>((resolve, reject) => {
    socket.once("connect", resolve);
    socket.once("error", reject);
  });
  const read = (async () => {
    for await (const data of Readable.toWeb(socket) as ReadableStream<Uint8Array>)
      process.stdout.write(data);
  })();
  const write = (async () => {
    for await (const data of Bun.stdin.stream()) {
      if (!socket.write(data)) await new Promise<void>((r) => socket.once("drain", r));
    }
    socket.end();
  })();
  try {
    await Promise.race([read, write]);
  } finally {
    socket.destroy();
  }
}
export async function requestJson(
  url: string,
  init: RequestInit = {},
  limit = 1024 * 1024,
): Promise<Json> {
  const response = await fetch(url, {
    ...init,
    signal: AbortSignal.timeout(8000),
    redirect: "error",
  });
  check(response.ok, "provider request failed");
  check(response.body, "provider response missing");
  return parse((await bounded(response.body, limit)).toString());
}
export const nativeModelId = (id: string): string => {
  const value = parse(id);
  check(
    Array.isArray(value) && value.length === 2 && typeof value[1] === "string",
    "invalid native model identity",
  );
  return value[1];
};
export const optionalObject = (value: unknown) => (value == null ? {} : object(value));
