import { expect, test } from "bun:test";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import type { ObjectValue } from "@kite/protocol";
import { RpcTimeoutError } from "@kite/runtime/rpc";
import { Ply } from "../src/native/ply";

test("Ply cancels an expired ACP prompt before another turn uses the same session", async () => {
  const events: ObjectValue[] = [];
  const ply = new Ply(
    {
      command: [process.execPath, resolve("packages/host/test/fixtures/native.ts"), "ply"],
      cwd: tmpdir(),
      model: "fixture-model",
      route: "fixture",
      event: (value) => events.push(value),
      permission: () => {
        throw new Error("unexpected permission request");
      },
      persist: () => {},
    },
    100,
  );
  try {
    await ply.initialize();
    ply.prepare();
    await expect(
      ply.run([{ type: "text", text: "wait" }], ply.currentModel, "high"),
    ).rejects.toBeInstanceOf(RpcTimeoutError);
    expect(ply.alive()).toBe(true);
    ply.prepare();
    await ply.run([{ type: "text", text: "complete" }], ply.currentModel, "high");
    expect(events.filter((value) => value.type === "assistant/message")).toEqual([
      expect.objectContaining({
        data: {
          message: { content: [{ type: "text", text: "native-2:complete:text:policy-ask" }] },
        },
      }),
    ]);
  } finally {
    ply.close();
    await ply.rpc.child.exited;
  }
});
