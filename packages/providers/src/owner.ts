import {
  appendFileSync,
  chmodSync,
  closeSync,
  existsSync,
  fsyncSync,
  openSync,
  rmSync,
} from "node:fs";
import { createServer, type Socket } from "node:net";
import { join } from "node:path";
import { Readable } from "node:stream";
import {
  check,
  type Json,
  json,
  type NativeAdapter,
  type NativeBlock,
  type ObjectValue,
  object,
  optionalString,
  parse,
  RPC_LIMIT,
  string,
} from "@kite/protocol";
import { atomic, lines, locked, readJson, same, startTicks } from "@kite/runtime";
import { RpcTimeoutError, TurnCancelled } from "@kite/runtime/rpc";
import { nativeModelId, optionalObject } from "@kite/runtime/socket";
import {
  type HarnessInfo,
  harnessDirectory,
  harnessInfo,
  migrationContext,
  modelIds,
} from "./harness";
import { connectNative } from "./native";
import { approvalFields, permissionValues } from "./permissions";
import { nativeInput } from "./uploads";

type Queued = [NativeBlock[], string, string, Record<string, string>];
const textContent = (blocks: Json | NativeBlock[] | undefined): string =>
  typeof blocks === "string"
    ? blocks
    : Array.isArray(blocks)
      ? blocks
          .map(object)
          .filter((b) => b.type === "text")
          .map((b) => optionalString(b.text))
          .join("\n")
      : "";
export class Owner {
  readonly info: HarnessInfo;
  readonly epoch = crypto.randomUUID();
  readonly clients = new Set<Socket>();
  private native: NativeAdapter | undefined;
  private pending = new Map<string, ObjectValue>();
  private queued: Queued[] = [];
  private selection: [string, string, Record<string, string>] | undefined;
  private turn: string | undefined;
  private lastTurn: string | undefined;
  private revision = 0;
  private cancelled = false;
  private appends = 0;
  private saved: Json | undefined;
  private migration: string | undefined;
  private tail: Promise<unknown> = Promise.resolve();
  constructor(readonly path: string) {
    this.info = harnessInfo(readJson(join(path, "session.json")));
    const saved = join(path, "native.json");
    this.saved = existsSync(saved) ? readJson(saved, RPC_LIMIT) : undefined;
    this.migration = this.saved === undefined ? migrationContext(path) : undefined;
  }
  private serialize<T>(fn: () => Promise<T> | T): Promise<T> {
    const next = this.tail.then(fn);
    this.tail = next.catch(() => {});
    return next;
  }
  async initialize(): Promise<void> {
    this.state("idle");
    await this.connect();
  }
  private async connect(): Promise<void> {
    const conf = {
      command: this.info.command,
      cwd: this.info.cwd,
      model: this.info.model,
      route: this.info.route,
      event: (v: ObjectValue) => this.event(v),
      permission: (v: ObjectValue) => this.permission(v),
      persist: (v: Json) => {
        this.saved = v;
        atomic(join(this.path, "native.json"), v);
      },
      ...(this.saved === undefined ? {} : { saved: this.saved }),
    };
    const native = await connectNative(this.info.provider, conf);
    try {
      const available = await modelIds(this.info.provider, this.info.model);
      native.models = native.models.filter((r) => available.has(nativeModelId(r.model)));
      check(
        native.models.some((r) => r.model === native.currentModel),
        "active harness model unavailable",
      );
      this.native = native;
    } catch (error) {
      native.close();
      throw error;
    }
  }
  private get adapter(): NativeAdapter {
    check(this.native, "native adapter unavailable");
    return this.native;
  }
  private send(socket: Socket, value: unknown): void {
    if (socket.destroyed) {
      this.clients.delete(socket);
      return;
    }
    const line = `${JSON.stringify(value)}\n`;
    if (
      Buffer.byteLength(line) > RPC_LIMIT ||
      socket.writableLength + Buffer.byteLength(line) > RPC_LIMIT
    ) {
      socket.destroy();
      this.clients.delete(socket);
      return;
    }
    socket.write(line);
  }
  private notify(method: string, params: ObjectValue): void {
    for (const client of this.clients)
      this.send(client, { method, params: { threadId: this.info.thread, ...params } });
  }
  private state(status: string): void {
    this.revision++;
    if (this.turn) this.lastTurn = this.turn;
    atomic(join(this.path, "state.json"), {
      protocol: 2,
      status,
      revision: this.revision,
      pid: process.pid,
      start_ticks: startTicks(process.pid),
      turn_id: this.lastTurn ?? null,
    });
  }
  private permission(request: ObjectValue): void {
    const id = `${this.epoch}:permission:${request.id}`;
    this.pending.set(id, request);
    this.state("blocked");
    if (this.pending.size === 1) this.offerPermission();
  }
  private offerPermission(): void {
    const first = this.pending.entries().next().value;
    if (!first) return;
    const [id, request] = first,
      tool = optionalObject(optionalObject(request.params).toolCall);
    for (const client of this.clients)
      this.send(client, {
        id,
        method: "item/commandExecution/requestApproval",
        params: {
          threadId: this.info.thread,
          turnId: this.turn,
          itemId: tool.toolCallId ?? id,
          ...approvalFields(tool, this.info.cwd),
          availableDecisions: ["accept", "decline"],
        },
      });
  }
  private record(id: string, payload: ObjectValue): void {
    const line = `${JSON.stringify({ id, type: "response_item", payload })}\n`;
    check(Buffer.byteLength(line) <= RPC_LIMIT, "harness event exceeds transport limit");
    const fd = openSync(join(this.path, "history.jsonl"), "a", 0o600);
    try {
      appendFileSync(fd, line);
      fsyncSync(fd);
    } finally {
      closeSync(fd);
    }
  }
  private message(role: string, text: string, id = crypto.randomUUID()): void {
    if (!text.trim()) return;
    this.record(id, { type: "message", role, content: [{ type: "text", text }] });
    if (role === "assistant")
      this.notify("item/completed", {
        turnId: this.turn,
        item: { id, type: "agentMessage", text },
      });
  }
  private event(value: ObjectValue): void {
    const data = optionalObject(value.data);
    if (value.type === "assistant/message")
      this.message(
        "assistant",
        textContent(optionalObject(data.message).content),
        `${this.epoch}:${value.seq ?? crypto.randomUUID()}`,
      );
    else if (value.type === "tool/call") {
      const id = `${this.turn}:${data.callId}`;
      this.record(id, {
        type: "function_call",
        call_id: id,
        name: data.name,
        arguments:
          typeof data.arguments === "string"
            ? data.arguments
            : JSON.stringify(data.arguments ?? {}),
      });
      this.notify("item/started", {
        turnId: this.turn,
        item: { id, type: "commandExecution", name: data.name ?? "Remote tool" },
      });
    } else if (value.type === "tool/result") {
      const m = optionalObject(data.message),
        id = `${this.turn}:${m.toolCallId ?? m.callId ?? ""}`;
      const output = textContent(m.content);
      this.record(`${id}:result`, {
        type: "function_call_output",
        call_id: id,
        output,
        failed: Boolean(m.isError),
      });
      this.notify("item/completed", {
        turnId: this.turn,
        item: {
          id,
          type: "commandExecution",
          status: m.isError ? "failed" : "completed",
          aggregatedOutput: output.slice(0, 128 * 1024),
          _meta: { outputTruncated: output.length > 128 * 1024 },
        },
      });
    }
  }
  private finish(turn: string, status: string, error: Json = null): void {
    this.turn = undefined;
    for (const id of this.pending.keys()) this.notify("serverRequest/resolved", { requestId: id });
    this.pending.clear();
    this.queued = [];
    atomic(join(this.path, "queued.json"), []);
    this.state(status === "failed" ? "failed" : status === "interrupted" ? "cancelled" : "done");
    this.notify("turn/completed", { turn: { id: turn, status, error } });
  }
  private async run(first: Queued, turn: string): Promise<void> {
    let current = first;
    try {
      while (true) {
        let [blocks, model, effort, settings] = current;
        await this.adapter.configure?.(settings);
        if (this.migration) {
          blocks = [
            {
              type: "text",
              text: `Earlier public conversation (context only):\n${this.migration}`,
            },
            ...blocks,
          ];
          this.migration = undefined;
        }
        const output = await this.adapter.run(blocks, model, effort);
        if (output !== undefined) this.message("assistant", output);
        const next = await this.serialize(() => {
          const pending = !this.cancelled ? this.queued.shift() : undefined;
          if (pending) {
            atomic(join(this.path, "queued.json"), this.queued);
            this.selection = [pending[1], pending[2], pending[3]];
            this.adapter.prepare();
            return pending;
          }
          this.finish(turn, this.cancelled ? "interrupted" : "completed");
          return undefined;
        });
        if (!next) return;
        current = next;
      }
    } catch (error) {
      await this.serialize(() =>
        this.finish(
          turn,
          error instanceof TurnCancelled ? "interrupted" : "failed",
          error instanceof TurnCancelled
            ? null
            : {
                message:
                  error instanceof RpcTimeoutError
                    ? "Harness turn timed out; inspect the conversation before continuing"
                    : "Harness turn failed; inspect the conversation before retrying",
              },
        ),
      );
    }
  }
  request(method: string, p: ObjectValue): Promise<Json> {
    return this.serialize(() =>
      ["initialize", "turn/start"].includes(method)
        ? locked(join(this.path, "owner.lock"), () => this.requestLocked(method, p))
        : this.requestLocked(method, p),
    );
  }
  private async requestLocked(method: string, p: ObjectValue): Promise<Json> {
    if (method === "initialize") {
      if (!this.adapter.alive()) {
        check(!this.turn, "native turn disconnected");
        this.adapter.close();
        await this.connect();
      }
      return { serverInfo: { name: "kite-harness", version: "2" } };
    }
    if (method === "initialized") return {};
    if (method === "model/list") return json({ data: this.adapter.models, nextCursor: null });
    if (method === "config/read")
      return json({
        config: {
          model: this.adapter.currentModel,
          model_reasoning_effort: this.adapter.currentEffort,
          session_settings: this.adapter.settings ?? [],
        },
      });
    if (method === "account/read") return { account: null, requiresOpenaiAuth: false };
    if (method === "thread/loaded/list") return { data: [this.info.thread] };
    check(p.threadId === this.info.thread, "harness thread mismatch");
    if (method === "thread/resume") {
      this.offerPermission();
      if (object(readJson(join(this.path, "state.json"))).status === "failed") {
        this.notify("error", {
          willRetry: false,
          error: { message: "The previous harness turn failed; inspect history before retrying" },
        });
        return { thread: { id: this.info.thread, status: { type: "systemError" } } };
      }
      return {
        thread: { id: this.info.thread, status: { type: this.turn ? "active" : "idle" } },
      };
    }
    if (method === "thread/turns/list") {
      return {
        data: this.turn ? [{ id: this.turn, status: "inProgress", items: [] }] : [],
        nextCursor: null,
      };
    }
    if (method === "turn/interrupt") {
      check(this.turn && p.turnId === this.turn, "turn expired");
      this.cancelled = true;
      this.queued = [];
      atomic(join(this.path, "queued.json"), []);
      this.adapter.cancel();
      return {};
    }
    check(method === "turn/start", "unsupported harness operation");
    const model = string(p.model),
      effort = optionalString(p.effort),
      row = this.adapter.models.find((r) => r.model === model);
    check(row, "model unavailable");
    const efforts = row.supportedReasoningEfforts.map((e) => e.reasoningEffort);
    check(efforts.includes(effort) || (!efforts.length && !effort), "reasoning effort unavailable");
    const blocks = nativeInput(p.input, row),
      prompt = textContent(blocks),
      settings = permissionValues(p.sessionSettings, this.adapter.settings ?? []);
    if (this.turn) {
      check(!this.cancelled && this.appends < 16, "pending input limit reached");
      const delivered =
        same(this.selection, [model, effort, settings]) &&
        !this.queued.length &&
        (await this.adapter.steer(blocks));
      if (!delivered) {
        const pending: Queued[] = [...this.queued, [blocks, model, effort, settings]];
        check(
          Buffer.byteLength(JSON.stringify(pending)) <= 36 * 1024 * 1024,
          "pending input exceeds limit",
        );
        this.queued = pending;
        atomic(join(this.path, "queued.json"), pending);
      }
      this.appends++;
      this.message("user", prompt);
      return { turn: { id: this.turn, status: "inProgress" } };
    }
    if (!this.adapter.alive()) {
      this.adapter.close();
      await this.connect();
    }
    this.adapter.prepare();
    this.selection = [model, effort, settings];
    this.cancelled = false;
    this.appends = 0;
    const turn = crypto.randomUUID();
    this.turn = turn;
    this.state("working");
    this.message("user", prompt, `${turn}:user`);
    queueMicrotask(() => {
      void this.run([blocks, model, effort, settings], turn);
    });
    return { turn: { id: turn, status: "inProgress" } };
  }
  private response(value: ObjectValue): Promise<void> {
    return this.serialize(async () => {
      const id = string(value.id);
      check(id === this.pending.keys().next().value, "approval expired");
      const decision = object(value.result).decision;
      check(decision === "accept" || decision === "decline", "invalid approval decision");
      const request = this.pending.get(id);
      check(request, "approval expired");
      await this.adapter.approve(request, decision === "accept");
      this.pending.delete(id);
      this.notify("serverRequest/resolved", { requestId: id });
      this.state(this.pending.size ? "blocked" : "working");
      this.offerPermission();
    });
  }
  attach(socket: Socket): void {
    this.clients.add(socket);
    socket.on("error", () => this.clients.delete(socket));
    socket.on("close", () => this.clients.delete(socket));
    void (async () => {
      try {
        for await (const line of lines(
          Readable.toWeb(socket) as ReadableStream<Uint8Array>,
          RPC_LIMIT,
        )) {
          let value: ObjectValue = {};
          try {
            value = object(parse(line));
            if (!value.method && value.id !== undefined) {
              await this.response(value);
              continue;
            }
            const result = await this.request(string(value.method), optionalObject(value.params));
            if (value.id !== undefined) this.send(socket, { id: value.id, result });
          } catch {
            if (value.id !== undefined)
              this.send(socket, {
                id: value.id,
                error: { code: -32602, message: "harness request rejected" },
              });
          }
        }
      } catch {
        /* only this client disconnects */
      } finally {
        this.clients.delete(socket);
        socket.destroy();
      }
    })();
  }
  close(): void {
    this.native?.close();
    for (const client of this.clients) client.destroy();
  }
}
export async function serveOwner(thread: string): Promise<void> {
  const path = harnessDirectory(thread),
    owner = new Owner(path);
  await owner.initialize();
  const socket = join(path, "owner.sock");
  rmSync(socket, { force: true });
  const server = createServer((s) => owner.attach(s));
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(socket, () => {
      chmodSync(socket, 0o600);
      resolve();
    });
  });
  const stop = () => {
    owner.close();
    server.close();
    rmSync(socket, { force: true });
    process.exit(0);
  };
  process.once("SIGTERM", stop);
  process.once("SIGINT", stop);
  await new Promise<void>((resolve) => server.once("close", resolve));
}
