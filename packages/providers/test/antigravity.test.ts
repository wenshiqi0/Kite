import { expect, spyOn, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Antigravity } from "../src/native/antigravity";

test("Antigravity waits for the native prompt result without a turn deadline", async () => {
  const root = mkdtempSync(join(tmpdir(), "kite-antigravity-test-"));
  const agy = new Antigravity({
    command: [
      "env",
      `HOME=${root}`,
      "KITE_FIXTURE_AGY_SIGNED_IN=1",
      process.execPath,
      resolve("packages/host/test/fixtures/antigravity.ts"),
    ],
    cwd: root,
    model: "gemini-flash",
    route: "fixture",
    event: () => {},
    permission: () => {
      throw new Error("unexpected permission request");
    },
    persist: () => {},
  });
  const call = spyOn(agy.rpc, "call");
  try {
    await agy.initialize();
    await agy.run([{ type: "text", text: "continue" }], agy.currentModel, agy.currentEffort);
    expect(call.mock.calls.find(([method]) => method === "session/new")?.[2]).toBe(15_000);
    expect(call.mock.calls.find(([method]) => method === "session/prompt")?.[2]).toBeNull();
  } finally {
    call.mockRestore();
    agy.close();
    await agy.rpc.child.exited;
    rmSync(root, { recursive: true, force: true });
  }
});
