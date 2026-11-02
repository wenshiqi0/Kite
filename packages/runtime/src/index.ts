import { dlopen, FFIType, ptr } from "bun:ffi";
import { createHash, randomUUID } from "node:crypto";
import {
  chmodSync,
  closeSync,
  constants,
  existsSync,
  fstatSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import { check, type Json, parse } from "@kite/protocol";
export const home = () => process.env.HOME || homedir();
export const sha256 = (value: string | Uint8Array) =>
  createHash("sha256").update(value).digest("hex");
export const sleep = (ms: number) => Bun.sleep(ms);
export function privateDir(path: string): string {
  mkdirSync(path, { recursive: true, mode: 0o700 });
  const info = lstatSync(path);
  check(info.isDirectory() && info.uid === process.getuid?.(), "invalid private directory");
  chmodSync(path, 0o700);
  return path;
}
export function atomic(path: string, value: unknown): void {
  atomicBytes(path, Buffer.from(JSON.stringify(value)));
}
export function atomicBytes(path: string, data: Uint8Array): void {
  const tmp = `${path}.${randomUUID()}.pending`;
  const fd = openSync(tmp, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, 0o600);
  try {
    writeFileSync(fd, data);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  try {
    renameSync(tmp, path);
    const dir = openSync(dirname(path), constants.O_RDONLY);
    try {
      fsyncSync(dir);
    } finally {
      closeSync(dir);
    }
  } finally {
    if (existsSync(tmp)) unlinkSync(tmp);
  }
}
export function readJson(path: string, limit = 1024 * 1024, privateFile = false): Json {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const info = fstatSync(fd);
    check(
      info.isFile() &&
        info.size <= limit &&
        (!privateFile || (info.uid === process.getuid?.() && !(info.mode & 0o077))),
      "invalid JSON file",
    );
    return parse(readFileSync(fd, "utf8"));
  } finally {
    closeSync(fd);
  }
}
const libc = () =>
  dlopen(process.platform === "darwin" ? "/usr/lib/libSystem.B.dylib" : "libc.so.6", {
    flock: { args: [FFIType.i32, FFIType.i32], returns: FFIType.i32 },
    getpgid: { args: [FFIType.i32], returns: FFIType.i32 },
    sysconf: { args: [FFIType.i32], returns: FFIType.i64 },
    execvp: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  });
export function tryLock(path: string): (() => void) | undefined {
  const fd = openSync(path, constants.O_CREAT | constants.O_RDWR | constants.O_NOFOLLOW, 0o600);
  const lib = libc();
  if (lib.symbols.flock(fd, 2 | 4) !== 0) {
    closeSync(fd);
    lib.close();
    return undefined;
  }
  return () => {
    lib.symbols.flock(fd, 8);
    closeSync(fd);
    lib.close();
  };
}
export async function locked<T>(
  path: string,
  fn: () => T | Promise<T>,
  timeout = 30_000,
): Promise<T> {
  const deadline = Date.now() + timeout;
  let release = tryLock(path);
  while (!release) {
    check(Date.now() < deadline, "lock timed out");
    await sleep(20);
    release = tryLock(path);
  }
  try {
    return await fn();
  } finally {
    release();
  }
}
export function pgid(pid: number): number {
  const lib = libc();
  try {
    return lib.symbols.getpgid(pid);
  } finally {
    lib.close();
  }
}
export function clockTicks(): number {
  const lib = libc();
  try {
    return Number(lib.symbols.sysconf(2));
  } finally {
    lib.close();
  }
}
export function exec(command: string[], env?: NodeJS.ProcessEnv): never {
  check(command.length > 0, "command required");
  if (env) {
    for (const key of Object.keys(process.env)) if (!(key in env)) delete process.env[key];
    Object.assign(process.env, env);
  }
  const buffers = command.map((s) => {
    check(!s.includes("\0"), "invalid argv");
    return Buffer.from(`${s}\0`);
  });
  const argv = new BigUint64Array([...buffers.map((b) => BigInt(ptr(b))), 0n]);
  const first = buffers[0];
  check(first, "command required");
  libc().symbols.execvp(ptr(first), ptr(argv));
  throw new Error("exec failed");
}
export function startTicks(pid: number): string | undefined {
  try {
    return readFileSync(`/proc/${pid}/stat`, "utf8").split(") ").at(-1)?.split(/\s+/)[19];
  } catch {
    return undefined;
  }
}
export function live(pid: number, ticks: string): boolean {
  try {
    return (
      startTicks(pid) === ticks &&
      !readFileSync(`/proc/${pid}/status`, "utf8").includes("State:\tZ")
    );
  } catch {
    return false;
  }
}
export function stopped(pid: number): boolean {
  return ["T", "t"].includes(
    readFileSync(`/proc/${pid}/stat`, "utf8").split(") ").at(-1)?.split(/\s+/)[0] || "",
  );
}
export function same(a: unknown, b: unknown): boolean {
  const normalize = (v: unknown): unknown =>
    Array.isArray(v)
      ? v.map(normalize)
      : v && typeof v === "object"
        ? Object.fromEntries(
            Object.entries(v)
              .sort(([a], [b]) => a.localeCompare(b))
              .map(([k, val]) => [k, normalize(val)]),
          )
        : v;
  return JSON.stringify(normalize(a)) === JSON.stringify(normalize(b));
}
export async function bounded(stream: ReadableStream<Uint8Array>, limit: number): Promise<Buffer> {
  const reader = stream.getReader();
  const rows: Uint8Array[] = [];
  let count = 0;
  try {
    while (true) {
      const r = await reader.read();
      if (r.done) break;
      count += r.value.byteLength;
      check(count <= limit, "stream exceeds limit");
      rows.push(r.value);
    }
    return Buffer.concat(rows);
  } catch (error) {
    await reader.cancel().catch(() => {});
    throw error;
  } finally {
    reader.releaseLock();
  }
}
export async function input(limit: number): Promise<Buffer> {
  return bounded(Bun.stdin.stream(), limit);
}
export async function* lines(
  stream: ReadableStream<Uint8Array>,
  limit: number,
): AsyncGenerator<string> {
  let pending = Buffer.alloc(0);
  for await (const data of stream) {
    pending = Buffer.concat([pending, data]);
    let at = pending.indexOf(10);
    while (at >= 0) {
      check(at <= limit, "line exceeds limit");
      yield pending.subarray(0, at).toString();
      pending = pending.subarray(at + 1);
      at = pending.indexOf(10);
    }
    check(pending.length <= limit, "line exceeds limit");
  }
  check(pending.length === 0, "incomplete JSONL record");
}
export interface RunOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  input?: string | Uint8Array;
  timeout?: number;
  limit?: number;
  allowFailure?: boolean;
}
export async function run(
  command: string[],
  options: RunOptions = {},
): Promise<{ stdout: Buffer; stderr: Buffer; code: number }> {
  const child = Bun.spawn(command, {
    stdin:
      options.input === undefined
        ? "ignore"
        : typeof options.input === "string"
          ? Buffer.from(options.input)
          : options.input,
    stdout: "pipe",
    stderr: "pipe",
    ...(options.cwd ? { cwd: options.cwd } : {}),
    ...(options.env ? { env: options.env } : {}),
  });
  const timer = setTimeout(() => child.kill("SIGKILL"), options.timeout ?? 8000);
  try {
    const [stdout, stderr, code] = await Promise.all([
      bounded(child.stdout, options.limit ?? 4 * 1024 * 1024),
      bounded(child.stderr, 65536),
      child.exited,
    ]);
    check(options.allowFailure || code === 0, "host command failed");
    return { stdout, stderr, code };
  } catch (error) {
    child.kill("SIGKILL");
    throw error;
  } finally {
    clearTimeout(timer);
  }
}
export function entry(): string {
  return resolve(process.env.KITE_HOST_BACKEND_PATH || process.argv[1] || "");
}
export const selfCommand = (...args: string[]) => [process.execPath, entry(), ...args];
export function requireOwnedFile(path: string, limit: number): Buffer {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const info = fstatSync(fd);
    check(
      info.isFile() && info.uid === process.getuid?.() && info.size > 0 && info.size <= limit,
      "invalid owned file",
    );
    const data = readFileSync(fd);
    check(data.length === info.size, "file changed during read");
    return data;
  } finally {
    closeSync(fd);
  }
}
export function directoryIsPrivate(path: string): boolean {
  const s = statSync(path);
  return s.isDirectory() && s.uid === process.getuid?.() && (s.mode & 0o077) === 0;
}
