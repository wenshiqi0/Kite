import {
  check,
  type Json,
  object,
  parse,
  RPC_LIMIT,
  type RpcMessage,
  rpcMessage,
} from "@kite/protocol";
import { lines } from "./index";
export class RpcError extends Error {
  constructor(public code: number) {
    super("native request rejected");
  }
}
export class TurnCancelled extends Error {}
export class RpcTimeoutError extends Error {
  constructor() {
    super("native request timed out; delivery may have occurred");
  }
}
export class RpcProcess {
  readonly child: Bun.Subprocess<"pipe", "pipe", "ignore">;
  private counter = 0;
  private pending = new Map<
    string | number,
    {
      resolve: (value: Json) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  private closed = false;
  constructor(
    command: string[],
    cwd: string,
    private notification: (value: RpcMessage) => void = () => {},
    private request: (value: RpcMessage) => void = () => {},
    env: NodeJS.ProcessEnv = process.env,
  ) {
    this.child = Bun.spawn(command, {
      cwd,
      env: { ...env },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "ignore",
      detached: true,
    });
    void this.read().catch(() => this.close());
    void this.child.exited.then(() => this.close());
  }
  write(value: RpcMessage): void {
    check(!this.closed, "native runtime is closed");
    const data = `${JSON.stringify({ jsonrpc: "2.0", ...value })}\n`;
    check(Buffer.byteLength(data) <= RPC_LIMIT, "native request exceeds limit");
    this.child.stdin.write(data);
    void this.child.stdin.flush();
  }
  call(
    method: string,
    params: Json,
    timeout = 40_000,
    onId?: (id: number) => void,
    cancelOnTimeout?: () => void,
  ): Promise<Json> {
    if (this.closed || this.child.exitCode !== null || this.child.signalCode !== null)
      return Promise.reject(new Error("native runtime is closed"));
    const id = ++this.counter;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const error = new RpcTimeoutError();
        const pending = this.pending.get(id);
        if (!pending) return;
        if (!cancelOnTimeout) {
          this.pending.delete(id);
          reject(error);
          return;
        }
        // Keep the request registered until native cancellation completes. A late
        // success still belongs to the expired request, never the next turn.
        pending.resolve = () => reject(error);
        pending.reject = () => reject(error);
        pending.timer = setTimeout(() => this.close(), 5000);
        try {
          cancelOnTimeout();
        } catch {
          this.close();
        }
      }, timeout);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.write({ id, method, params });
        onId?.(id);
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }
  private async read(): Promise<void> {
    for await (const line of lines(this.child.stdout, RPC_LIMIT)) {
      const value = object(parse(line));
      if (value.id !== undefined && !value.method) {
        check(typeof value.id === "string" || typeof value.id === "number", "invalid RPC ID");
        const p = this.pending.get(value.id);
        if (!p) continue;
        this.pending.delete(value.id);
        clearTimeout(p.timer);
        if (value.error) p.reject(new RpcError(Number(object(value.error).code)));
        else p.resolve(value.result ?? {});
      } else if (typeof value.method === "string") {
        const message = rpcMessage(value);
        if (value.id !== undefined) this.request(message);
        else this.notification(message);
      }
    }
  }
  close(): void {
    if (this.closed) return;
    this.closed = true;
    for (const p of this.pending.values()) {
      clearTimeout(p.timer);
      p.reject(new Error("native runtime disconnected"));
    }
    this.pending.clear();
    this.child.stdin.end();
    if (this.child.exitCode === null && this.child.signalCode === null) {
      try {
        process.kill(-this.child.pid, "SIGTERM");
      } catch {
        this.child.kill();
      }
    }
  }
}
