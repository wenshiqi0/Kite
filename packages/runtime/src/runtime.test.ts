import { afterAll, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { atomic, bounded, locked, run, startTicks, tryLock } from "./index";

const root = mkdtempSync(join(tmpdir(), "kite-runtime-test-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
test("advisory locks exclude other processes and release after failures", async () => {
  const path = join(root, "owned.lock");
  const release = tryLock(path);
  expect(release).toBeDefined();
  try {
    const child = await run([
      process.execPath,
      "-e",
      `import {tryLock} from ${JSON.stringify(join(import.meta.dir, "index.ts"))};const release=tryLock(${JSON.stringify(path)});process.exit(release?1:0);`,
    ]);
    expect(child.code).toBe(0);
  } finally {
    release?.();
  }
  await expect(
    locked(path, () => {
      throw new Error("owned failure");
    }),
  ).rejects.toThrow("owned failure");
  const again = tryLock(path);
  expect(again).toBeDefined();
  again?.();
});
test("atomic persistence replaces the complete file with private permissions", () => {
  const file = join(root, "state.json");
  atomic(file, { revision: 1 });
  atomic(file, { revision: 2 });
  expect(JSON.parse(readFileSync(file, "utf8"))).toEqual({ revision: 2 });
  expect(existsSync(file)).toBe(true);
});
test("bounded output stops an excessive or hung process", async () => {
  await expect(
    run([process.execPath, "-e", "process.stdout.write('x'.repeat(4096))"], { limit: 128 }),
  ).rejects.toThrow("limit");
  await expect(
    run([process.execPath, "-e", "setInterval(()=>{},1000)"], { timeout: 30 }),
  ).rejects.toThrow("failed");
  const stream = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new Uint8Array(10));
      c.close();
    },
  });
  await expect(bounded(stream, 5)).rejects.toThrow("limit");
});
test("Linux owner identity includes the live start time", () => {
  expect(startTicks(process.pid)).toMatch(/^\d+$/);
  expect(startTicks(2147483647)).toBeUndefined();
});
