import { expect, test } from "bun:test";
import { tmpdir } from "node:os";
import { RpcProcess, RpcTimeoutError } from "../src/rpc";

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

test("an unacknowledged timeout cancels once and closes the native process", async () => {
  const rpc = new RpcProcess(
    [process.execPath, "-e", "process.stdin.resume(); setInterval(() => {}, 1000)"],
    tmpdir(),
  );
  let cancellations = 0;
  try {
    await expect(
      rpc.call("begin", {}, 20, undefined, () => {
        cancellations++;
        rpc.write({ method: "cancel" });
      }),
    ).rejects.toBeInstanceOf(RpcTimeoutError);
    await rpc.child.exited;
    expect(cancellations).toBe(1);
    expect(rpc.child.signalCode).toBe("SIGTERM");
    expect(() => process.kill(rpc.child.pid, 0)).toThrow();
    await expect(rpc.call("begin", {})).rejects.toThrow("closed");
  } finally {
    rpc.close();
  }
}, 10000);
