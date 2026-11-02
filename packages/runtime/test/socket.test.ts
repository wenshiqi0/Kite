import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { listening, requirePrivateSocket } from "../src/socket";

test("private sockets and one owned alias are accepted; public endpoints and alias chains are refused", async () => {
  const root = mkdtempSync(join(tmpdir(), "kite-sock-"));
  chmodSync(root, 0o700);
  const path = join(root, "s"),
    alias = join(root, "a"),
    chain = join(root, "b"),
    server = createServer((s) => s.end());
  try {
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(path, resolve);
    });
    chmodSync(path, 0o600);
    symlinkSync("s", alias);
    symlinkSync("a", chain);
    requirePrivateSocket(path);
    requirePrivateSocket(alias);
    expect(await listening(alias)).toBe(true);
    expect(() => requirePrivateSocket(chain)).toThrow("not private");
    chmodSync(path, 0o666);
    expect(() => requirePrivateSocket(alias)).toThrow("not private");
    chmodSync(path, 0o600);
    chmodSync(root, 0o755);
    expect(() => requirePrivateSocket(alias)).toThrow("not private");
    chmodSync(root, 0o700);
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
    rmSync(root, { recursive: true, force: true });
  }
});
test("a stale socket path or regular file is not a live backend", async () => {
  const root = mkdtempSync(join(tmpdir(), "kite-stale-")),
    path = join(root, "s");
  try {
    writeFileSync(path, "owned fixture");
    expect(await listening(path)).toBe(false);
    expect(() => requirePrivateSocket(path)).toThrow("not private");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
