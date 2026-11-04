/** ACP is another client of the existing Kite owners, never another model loop. */
import { realpathSync } from "node:fs";
import { isAbsolute, join, relative } from "node:path";
import {
  array,
  check,
  type Json,
  json,
  type ObjectValue,
  object,
  optionalString,
  type RpcMessage,
  type SessionRecord,
  string,
} from "@kite/protocol";
import { home, locked, run, selfCommand } from "@kite/runtime";
import type { AcpConnection } from "@kite/runtime/acp";
import { RpcProcess } from "@kite/runtime/rpc";
import { optionalObject } from "@kite/runtime/socket";
import { catalogRead, catalogRoot, catalogUnlocked, saveCatalogUnlocked } from "./catalog";
import { sessionOpen } from "./codex";

export interface AcpBackend {
  thread: string;
  model: string;
  effort: string;
  call(method: string, params: Json): Promise<Json>;
  write(message: RpcMessage): void;
  close(): void;
}
export interface AcpRepository {
  list(): Promise<SessionRecord[]>;
  create(provider: string, cwd: string, title: string): Promise<SessionRecord>;
}
export interface KiteAcpOptions {
  machine: string;
  workspaces: string[];
  herdr: string;
  defaultProvider: "codex" | "ply" | "dsh";
  newSessionProviders?: string[];
  repository?: AcpRepository;
  backend?(
    record: SessionRecord,
    event: (message: RpcMessage) => void,
    request: (message: RpcMessage) => void,
  ): Promise<AcpBackend>;
}
const supported = (provider: string) => ["codex", "ply", "dsh"].includes(provider);
const repository: AcpRepository = {
  list: async () => (await catalogRead()).sessions,
  create: async (provider, cwd, title) =>
    locked(join(catalogRoot(), "sessions.lock"), () => {
      check(
        provider === "codex" || provider === "ply" || provider === "dsh",
        "network session provider is not supported",
      );
      const catalog = catalogUnlocked(),
        at = Date.now();
      check(catalog.sessions.length < 1024, "session catalog is full");
      const record: SessionRecord = {
        id: crypto.randomUUID().replaceAll("-", ""),
        provider,
        cwd,
        title,
        automatic_title: !title,
        preview: "",
        archived: false,
        reference: null,
        created_ms: at,
        updated_ms: at,
      };
      catalog.sessions.push(record);
      saveCatalogUnlocked(catalog);
      return record;
    }),
};

async function connectBackend(
  record: SessionRecord,
  herdr: string,
  event: (message: RpcMessage) => void,
  request: (message: RpcMessage) => void,
): Promise<AcpBackend> {
  const agent = object(object(await sessionOpen(record.id, herdr)).agent),
    reference = object(agent.agent_session),
    thread = string(reference.value);
  check(
    reference.kind === "id" &&
      reference.agent === record.provider &&
      supported(string(reference.agent)),
    "structured owner identity unavailable",
  );
  const rpc = new RpcProcess(
    selfCommand("proxy", "--pane", string(agent.pane_id)),
    record.cwd || home(),
    event,
    request,
  );
  try {
    await rpc.call("initialize", {
      clientInfo: { name: "kite-acp", version: "1" },
      capabilities: { experimentalApi: true },
    });
    rpc.write({ method: "initialized" });
    const models = array(
        object(await rpc.call("model/list", { limit: 100, includeHidden: false })).data,
      ).map(object),
      config = optionalObject(object(await rpc.call("config/read", {})).config),
      model =
        models.find((m) => m.model === config.model) ??
        models.find((m) => m.isDefault === true) ??
        models[0];
    check(model, "owner model unavailable");
    const backend: AcpBackend = {
      thread,
      model: string(model.model),
      effort:
        optionalString(config.model_reasoning_effort) ||
        optionalString(model.defaultReasoningEffort),
      call: (method, params) => rpc.call(method, params),
      write: (message) => rpc.write(message),
      close: () => rpc.close(),
    };
    void rpc.child.exited.then(() =>
      event({ method: "_kite/backend_closed", params: { threadId: thread } }),
    );
    const resumed = object(
      object(await backend.call("thread/resume", { threadId: thread, excludeTurns: true })).thread,
    );
    check(resumed.id === thread, "resumed owner identity mismatch");
    return backend;
  } catch (error) {
    rpc.close();
    throw error;
  }
}

interface PendingTurn {
  id?: string;
  resolve(value: Json): void;
  reject(error: Error): void;
  timer: ReturnType<typeof setTimeout>;
  completed?: ObjectValue;
}
interface Binding {
  record: SessionRecord;
  backend: AcpBackend;
  turn?: PendingTurn | undefined;
  prompting?: boolean;
  cancelRequested?: boolean;
  deltas: Set<string>;
}
interface Permission {
  binding: Binding;
  nativeId: string | number | null;
  turn: string;
  options: Map<string, boolean>;
}

export class KiteAcp implements AcpConnection {
  private initialized = false;
  private closed = false;
  private bindings = new Map<string, Binding>();
  private permissions = new Map<string, Permission>();
  private roots: string[];
  private repository: AcpRepository;
  private sessionTail: Promise<unknown> = Promise.resolve();
  constructor(
    private options: KiteAcpOptions,
    private send: (message: RpcMessage) => void,
  ) {
    check(
      options.workspaces.length > 0 && options.workspaces.length <= 32,
      "ACP requires explicit workspace roots",
    );
    this.roots = options.workspaces.map((path) => realpathSync(path));
    this.repository = options.repository ?? repository;
  }
  private cwd(path: string): string {
    check(isAbsolute(path), "ACP working directory must be absolute");
    const actual = realpathSync(path);
    check(
      this.roots.some((root) => {
        const child = relative(root, actual);
        return child === "" || (!child.startsWith("..") && !isAbsolute(child));
      }),
      "ACP working directory is outside configured workspaces",
    );
    return actual;
  }
  private publish(sessionId: string, update: Json): void {
    if (!this.closed) this.send({ method: "session/update", params: { sessionId, update } });
  }
  private finish(binding: Binding, turn: ObjectValue): void {
    const pending = binding.turn;
    if (!pending) return;
    if (!pending.id) {
      pending.completed = turn;
      return;
    }
    if (pending.id !== turn.id) return;
    clearTimeout(pending.timer);
    binding.turn = undefined;
    for (const [id, permission] of this.permissions)
      if (permission.binding === binding) this.permissions.delete(id);
    if (turn.status === "failed")
      pending.reject(new Error("remote agent turn failed; inspect its persisted conversation"));
    else
      pending.resolve({
        stopReason: turn.status === "interrupted" ? "cancelled" : "end_turn",
        _meta: {
          kite: {
            machine: this.options.machine,
            sessionId: binding.record.id,
            threadId: binding.backend.thread,
          },
        },
      });
  }
  private event(binding: Binding, message: RpcMessage): void {
    const params = optionalObject(message.params);
    if (params.threadId !== binding.backend.thread) return;
    if (message.method === "_kite/backend_closed") {
      if (binding.turn) {
        clearTimeout(binding.turn.timer);
        binding.turn.reject(new Error("owner transport disconnected; remote work may continue"));
        binding.turn = undefined;
      }
      return;
    }
    if (message.method === "turn/completed") {
      this.finish(binding, object(params.turn));
      return;
    }
    if (message.method === "serverRequest/resolved") {
      for (const [id, permission] of this.permissions)
        if (permission.binding === binding && permission.nativeId === params.requestId)
          this.permissions.delete(id);
      return;
    }
    const item = optionalObject(params.item),
      id = optionalString(params.itemId) || optionalString(item.id);
    if (message.method === "item/agentMessage/delta") {
      binding.deltas.add(id);
      this.publish(binding.record.id, {
        sessionUpdate: "agent_message_chunk",
        messageId: id,
        content: { type: "text", text: optionalString(params.delta) },
      });
    } else if (message.method === "item/completed" && item.type === "agentMessage") {
      if (!binding.deltas.delete(id))
        this.publish(binding.record.id, {
          sessionUpdate: "agent_message_chunk",
          messageId: id,
          content: { type: "text", text: optionalString(item.text) },
        });
    } else if (
      ["item/started", "item/completed"].includes(message.method || "") &&
      ["commandExecution", "fileChange", "mcpToolCall", "dynamicToolCall", "webSearch"].includes(
        optionalString(item.type),
      )
    ) {
      const output = optionalString(item.aggregatedOutput) || optionalString(item.output);
      this.publish(binding.record.id, {
        sessionUpdate: message.method === "item/started" ? "tool_call" : "tool_call_update",
        toolCallId: id,
        title:
          optionalString(item.command) || optionalString(item.name) || optionalString(item.type),
        status:
          message.method === "item/started"
            ? "in_progress"
            : item.status === "failed" ||
                item.failed === true ||
                (typeof item.exitCode === "number" && item.exitCode !== 0)
              ? "failed"
              : "completed",
        content: output
          ? [{ type: "content", content: { type: "text", text: output.slice(0, 128 * 1024) } }]
          : [],
        _meta: {
          kite: {
            outputTruncated:
              output.length > 128 * 1024 || optionalObject(item._meta).outputTruncated === true,
          },
        },
      });
    }
  }
  private permission(binding: Binding, message: RpcMessage): void {
    const params = optionalObject(message.params);
    if (params.threadId !== binding.backend.thread || message.id === undefined) return;
    if (!message.method?.endsWith("requestApproval")) {
      if (binding.turn)
        binding.backend.write({
          id: message.id,
          error: {
            code: -32601,
            message: "This interaction is not supported by the network ACP endpoint",
          },
        });
      return;
    }
    const turn = string(params.turnId),
      id = `permission-${crypto.randomUUID()}`,
      options = [
        { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
        { optionId: "reject-once", name: "Reject", kind: "reject_once" },
      ];
    check(this.permissions.size < 32, "ACP permission queue is full");
    this.permissions.set(id, {
      binding,
      nativeId: message.id,
      turn,
      options: new Map([
        ["allow-once", true],
        ["reject-once", false],
      ]),
    });
    this.send({
      id,
      method: "session/request_permission",
      params: {
        sessionId: binding.record.id,
        toolCall: {
          toolCallId: optionalString(params.itemId) || id,
          title: optionalString(params.command) || "Remote tool operation",
          kind: message.method.includes("fileChange") ? "edit" : "execute",
          rawInput: json(params),
          locations: [{ path: binding.record.cwd || "" }],
          status: "pending",
        },
        options,
      },
    });
  }
  private async bind(id: string, path: string): Promise<Binding> {
    const existing = this.bindings.get(id);
    if (existing) {
      check(existing.record.cwd === path, "session working directory mismatch");
      return existing;
    }
    check(this.bindings.size < 16, "ACP connection session limit reached");
    const record = (await this.repository.list()).find((row) => row.id === id);
    check(
      record &&
        supported(record.provider) &&
        !record.archived &&
        this.cwd(record.cwd || "") === path,
      "session unavailable in this workspace",
    );
    let binding: Binding | undefined;
    const deferred: ["event" | "request", RpcMessage][] = [];
    const event = (message: RpcMessage) => {
        if (binding) this.event(binding, message);
        else deferred.push(["event", message]);
      },
      request = (message: RpcMessage) => {
        if (binding) this.permission(binding, message);
        else deferred.push(["request", message]);
      };
    const backend = await (this.options.backend
      ? this.options.backend(record, event, request)
      : connectBackend(record, this.options.herdr, event, request));
    if (this.closed) {
      backend.close();
      throw new Error("ACP connection closed");
    }
    binding = { record, backend, deltas: new Set() };
    this.bindings.set(id, binding);
    for (const [kind, message] of deferred)
      if (kind === "event") this.event(binding, message);
      else this.permission(binding, message);
    return binding;
  }
  private async dispatch(method: string, params: ObjectValue): Promise<Json> {
    if (method === "initialize") {
      check(!this.initialized && params.protocolVersion === 1, "ACP v1 initialization required");
      this.initialized = true;
      return {
        protocolVersion: 1,
        agentInfo: { name: "kite", title: this.options.machine, version: "1" },
        agentCapabilities: { loadSession: false, sessionCapabilities: { list: {}, resume: {} } },
        authMethods: [],
        _meta: {
          kite: {
            machine: this.options.machine,
            transport: "websocket-v1",
            newSessionProviders: this.options.newSessionProviders ?? ["codex", "ply", "dsh"],
            resumedSessionProviders: ["codex", "ply", "dsh"],
          },
        },
      };
    }
    check(this.initialized, "initialize the ACP connection first");
    if (method === "initialized") return {};
    if (method === "session/list") {
      const rows = (await this.repository.list()).filter((r) => {
        try {
          return supported(r.provider) && !r.archived && Boolean(this.cwd(r.cwd || ""));
        } catch {
          return false;
        }
      });
      const cursor = optionalString(params.cursor) || "0";
      check(/^\d{1,6}$/.test(cursor) && Number(cursor) <= rows.length, "invalid session cursor");
      const start = Number(cursor),
        end = Math.min(start + 100, rows.length);
      return {
        sessions: rows.slice(start, end).map((r) => ({
          sessionId: r.id,
          cwd: r.cwd || "",
          title: r.title,
          updatedAt: new Date(Number(r.updated_ms) || 0).toISOString(),
          _meta: { kite: { machine: this.options.machine, provider: r.provider } },
        })),
        ...(end < rows.length ? { nextCursor: String(end) } : {}),
      };
    }
    if (method === "session/new" || method === "session/resume") {
      const create = async (): Promise<Json> => {
        check(!this.closed && this.bindings.size < 16, "ACP session limit or closed connection");
        const cwd = this.cwd(string(params.cwd));
        check(
          Array.isArray(params.mcpServers) && !params.mcpServers.length,
          "client-provided MCP servers are not available on this endpoint",
        );
        const meta = optionalObject(optionalObject(params._meta).kite),
          provider = optionalString(meta.provider) || this.options.defaultProvider;
        if (method === "session/new")
          check(
            (this.options.newSessionProviders ?? ["codex", "ply", "dsh"]).includes(provider),
            "network provider is not available on this host",
          );
        const record =
            method === "session/new"
              ? await this.repository.create(
                  provider,
                  cwd,
                  optionalString(meta.title).slice(0, 512),
                )
              : undefined,
          id = record?.id || string(params.sessionId);
        const binding = await this.bind(id, cwd);
        return {
          sessionId: id,
          _meta: {
            kite: {
              machine: this.options.machine,
              provider: binding.record.provider,
              threadId: binding.backend.thread,
            },
          },
        };
      };
      const next = this.sessionTail.then(create);
      this.sessionTail = next.catch(() => {});
      return next;
    }
    const id = string(params.sessionId),
      binding = this.bindings.get(id);
    check(binding, "session is not attached to this ACP connection");
    if (method === "session/cancel") {
      if (binding.prompting && !binding.turn?.id) {
        binding.cancelRequested = true;
        return {};
      }
      check(binding.turn?.id, "no prompt owned by this connection is active");
      await binding.backend.call("turn/interrupt", {
        threadId: binding.backend.thread,
        turnId: binding.turn.id,
      });
      return {};
    }
    check(method === "session/prompt", "ACP method not supported");
    check(!binding.turn && !binding.prompting, "session prompt is already active");
    const blocks = array(params.prompt).map(object);
    check(
      blocks.length > 0 &&
        blocks.length <= 64 &&
        blocks.every(
          (b) =>
            (b.type === "text" && typeof b.text === "string") ||
            (b.type === "resource_link" && typeof b.uri === "string"),
        ),
      "unsupported ACP prompt content",
    );
    binding.prompting = true;
    binding.cancelRequested = false;
    try {
      const meta = optionalObject(optionalObject(params._meta).kite);
      if (meta.expectedRevision !== undefined) {
        const expected = string(meta.expectedRevision);
        check(/^[a-f0-9]{40}$/.test(expected), "invalid expected source revision");
        const current = await run(["git", "-C", string(binding.record.cwd), "rev-parse", "HEAD"], {
          timeout: 5000,
        });
        check(
          current.stdout.toString().trim() === expected,
          "remote workspace source revision differs from the requested build",
        );
        const dirty = await run(
          [
            "git",
            "-C",
            string(binding.record.cwd),
            "status",
            "--porcelain",
            "--untracked-files=normal",
          ],
          { timeout: 5000 },
        );
        check(!dirty.stdout.length, "remote build workspace contains tracked modifications");
      }
      const turns = array(
        object(
          await binding.backend.call("thread/turns/list", {
            threadId: binding.backend.thread,
            limit: 1,
            itemsView: "notLoaded",
            sortDirection: "desc",
          }),
        ).data,
      ).map(object);
      check(turns[0]?.status !== "inProgress", "remote session already has an active turn");
      if (binding.cancelRequested) return { stopReason: "cancelled" };
      const { promise: result, resolve, reject } = Promise.withResolvers<Json>();
      const pending: PendingTurn = {
        resolve,
        reject,
        timer: setTimeout(() => {
          if (binding.turn === pending) binding.turn = undefined;
          reject(new Error("ACP prompt timed out; remote work may continue, do not replay it"));
        }, 10 * 60_000),
      };
      binding.turn = pending;
      try {
        const started = object(
          object(
            await binding.backend.call("turn/start", {
              threadId: binding.backend.thread,
              input: blocks.map((b) => ({
                type: "text",
                text:
                  b.type === "text"
                    ? string(b.text)
                    : `${optionalString(b.name)}: ${string(b.uri)}`,
              })),
              model: binding.backend.model,
              effort: binding.backend.effort,
            }),
          ).turn,
        );
        check(binding.turn, "prompt ended before acknowledgement");
        pending.id = string(started.id);
        if (pending.completed) this.finish(binding, pending.completed);
        else if (binding.cancelRequested)
          await binding.backend.call("turn/interrupt", {
            threadId: binding.backend.thread,
            turnId: string(started.id),
          });
      } catch (error) {
        const pending = binding.turn;
        binding.turn = undefined;
        if (pending) {
          clearTimeout(pending.timer);
          pending.reject(error instanceof Error ? error : new Error("remote prompt failed"));
        }
      }
      return await result;
    } finally {
      binding.prompting = false;
    }
  }
  async receive(message: RpcMessage): Promise<void> {
    if (!message.method) {
      const permission =
        typeof message.id === "string" ? this.permissions.get(message.id) : undefined;
      if (!permission) return;
      this.permissions.delete(string(message.id));
      const outcome = optionalObject(optionalObject(message.result).outcome),
        selected = optionalString(outcome.optionId);
      const allow = outcome.outcome === "selected" && permission.options.get(selected) === true;
      permission.binding.backend.write({
        id: permission.nativeId,
        result: { decision: allow ? "accept" : "decline" },
      });
      return;
    }
    try {
      const result = await this.dispatch(message.method, optionalObject(message.params));
      if (message.id !== undefined && !this.closed) this.send({ id: message.id, result });
    } catch (error) {
      if (message.id !== undefined && !this.closed)
        this.send({
          id: message.id,
          error: {
            code: -32602,
            message: error instanceof Error ? error.message : "ACP request rejected",
          },
        });
    }
  }
  close(): void {
    this.closed = true;
    for (const binding of this.bindings.values()) {
      binding.backend.close();
      if (binding.turn) {
        clearTimeout(binding.turn.timer);
        binding.turn.reject(new Error("ACP disconnected; remote work is not replayed"));
      }
    }
    this.bindings.clear();
    this.permissions.clear();
  }
}
