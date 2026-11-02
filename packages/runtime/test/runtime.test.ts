import { afterAll, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  atomic,
  bounded,
  live,
  locked,
  processContext,
  run,
  startTicks,
  stopped,
  tryLock,
} from "../src/index";

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
      `import {tryLock} from ${JSON.stringify(join(import.meta.dir, "../src/index.ts"))};const release=tryLock(${JSON.stringify(path)});process.exit(release?1:0);`,
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
test("owner identity includes the live start time", () => {
  expect(startTicks(process.pid)).toMatch(/^\d+$/);
  expect(startTicks(2147483647)).toBeUndefined();
});
test("owned process context preserves argv, cwd and environment on the actual platform", async () => {
  const output = await run(
    [
      process.execPath,
      "-e",
      `import {processContext,startTicks,live,stopped} from ${JSON.stringify(join(import.meta.dir, "../src/index.ts"))}; const c=processContext(process.pid); console.log(JSON.stringify({argv:c.argv.slice(-2),cwd:c.cwd,marker:c.environment.KITE_OWNED_PROCESS_TEST,live:live(process.pid,startTicks(process.pid)),stale:live(process.pid,"1"),stopped:stopped(process.pid)}))`,
      "space argument",
      "中文",
    ],
    { cwd: root, env: { ...process.env, KITE_OWNED_PROCESS_TEST: "owned-fixture" } },
  );
  expect(JSON.parse(output.stdout.toString())).toEqual({
    argv: ["space argument", "中文"],
    cwd: realpathSync(root),
    marker: "owned-fixture",
    live: true,
    stale: false,
    stopped: false,
  });
  expect(processContext(process.pid).argv.length).toBeGreaterThan(0);
  expect(live(2147483647, "1")).toBe(false);
  expect(stopped(process.pid)).toBe(false);
});
test.skipIf(process.platform !== "darwin")(
  "Darwin protected shells expose cwd and argv even when environment is omitted",
  async () => {
    const child = Bun.spawn(["/bin/zsh", "-f"], {
      stdin: "pipe",
      stdout: "ignore",
      stderr: "ignore",
      env: { ...process.env, KITE_OWNED_PROCESS_TEST: "shell-fixture" },
      cwd: root,
    });
    try {
      await Bun.sleep(100);
      const context = processContext(child.pid);
      expect(context.cwd).toBe(realpathSync(root));
      expect(context.argv[0]).toContain("zsh");
    } finally {
      child.kill();
      await child.exited;
    }
  },
);
