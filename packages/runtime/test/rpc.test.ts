import { expect, spyOn, test } from "bun:test";
import { tmpdir } from "node:os";
import { RpcProcess, RpcTimeoutError, TurnCancelled } from "../src/rpc";

test("a request without a deadline stays pending until the native reply", async () => {
  const peer = `
    import { createInterface } from "node:readline";
    let active;
    createInterface({ input: process.stdin }).on("line", line => {
      const request = JSON.parse(line);
      if (request.method === "begin") active = request.id;
      if (request.method === "finish") {
        console.log(JSON.stringify({ id: active, result: "finished" }));
        console.log(JSON.stringify({ id: request.id, result: {} }));
      }
    });
  `;
  const rpc = new RpcProcess([process.execPath, "-e", peer], tmpdir());
  const timers = spyOn(globalThis, "setTimeout");
  const pending = rpc.call("begin", {}, null);
  void pending.catch(() => {});
  const timerCount = timers.mock.calls.length;
  timers.mockRestore();
  try {
    expect(timerCount).toBe(0);
    await expect(rpc.call("control", {}, 20)).rejects.toBeInstanceOf(RpcTimeoutError);
    await rpc.call("finish", {});
    await expect(pending).resolves.toBe("finished");
  } finally {
    rpc.close();
    await rpc.child.exited;
  }
});

test("an expired request waits for native cancellation and rejects a late success", async () => {
  const peer = `
    import { createInterface } from "node:readline";
    let active;
    createInterface({ input: process.stdin }).on("line", line => {
      const request = JSON.parse(line);
      const send = result => console.log(JSON.stringify({ id: request.id, result }));
      if (request.method === "begin") active = request.id;
      else if (request.method === "cancel") setTimeout(() => {
        console.log(JSON.stringify({ id: active, result: "late success" }));
        active = undefined;
      }, 30);
      else send({ idle: active === undefined });
    });
  `;
  const rpc = new RpcProcess([process.execPath, "-e", peer], tmpdir());
  let cancellations = 0;
  try {
    await expect(
      rpc.call("begin", {}, 50, undefined, () => {
        cancellations++;
        rpc.write({ method: "cancel" });
      }),
    ).rejects.toBeInstanceOf(RpcTimeoutError);
    expect(cancellations).toBe(1);
    expect(await rpc.call("status", {})).toEqual({ idle: true });
    expect(rpc.child.exitCode).toBeNull();
  } finally {
    rpc.close();
    await rpc.child.exited;
  }
});

test.each(["timeout", "explicit cancellation"])(
  "unacknowledged %s closes the native process",
  async (kind) => {
    const rpc = new RpcProcess(
      [process.execPath, "-e", "process.stdin.resume(); setInterval(() => {}, 1000)"],
      tmpdir(),
    );
    let cancellations = 0;
    const sendCancel = () => {
      cancellations++;
      rpc.write({ method: "cancel" });
    };
    try {
      const pending =
        kind === "timeout"
          ? rpc.call("begin", {}, 20, undefined, sendCancel)
          : rpc.call("begin", {}, null, (id) => {
              rpc.cancel(id, sendCancel);
              rpc.cancel(id, sendCancel);
            });
      await expect(pending).rejects.toBeInstanceOf(
        kind === "timeout" ? RpcTimeoutError : TurnCancelled,
      );
      await rpc.child.exited;
      expect(cancellations).toBe(1);
      expect(rpc.alive()).toBe(false);
      expect(rpc.child.signalCode).toBe("SIGTERM");
      expect(() => process.kill(rpc.child.pid, 0)).toThrow();
      await expect(rpc.call("begin", {})).rejects.toThrow("closed");
    } finally {
      rpc.close();
    }
  },
  10000,
);
