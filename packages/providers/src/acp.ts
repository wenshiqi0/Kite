/** ACP is another client of the existing Kite owners, never another model loop. */
import { join } from "node:path";
import {
  array,
  boundedString,
  CHAT_PROVIDERS,
  type ChatProvider,
  check,
  isChatProvider,
  type Json,
  json,
  type ObjectValue,
  object,
  optionalString,
  type RpcMessage,
  type SessionDelegation,
  type SessionRecord,
  type SessionSelection,
  string,
} from "@kite/protocol";
import {
  ACP_CONTROL_METHODS,
  ACP_MAX_CHILDREN_PER_PARENT,
  ACP_PLACEMENT_METHOD,
  acpSessionId,
} from "@kite/protocol/acp";
import { locked } from "@kite/runtime";
import type { AcpConnection } from "@kite/runtime/acp";
import { optionalObject } from "@kite/runtime/socket";
import { type AcpBackend, connectBackend, selectModel, verifyRevision } from "./acp-backend";
import { AcpControl } from "./acp-control";
import { acpTurnSettings, type OwnerStatus, ownerPresent } from "./acp-owner";
import {
  catalogRead,
  catalogRoot,
  catalogUnlocked,
  patchSession,
  saveCatalogUnlocked,
} from "./catalog";

export type { AcpBackend } from "./acp-backend";

export interface AcpRepository {
  list(): Promise<SessionRecord[]>;
  create(
    provider: string,
    cwd: string,
    title: string,
    delegation?: SessionDelegation,
    selection?: SessionSelection,
  ): Promise<SessionRecord>;
  configure?(id: string, selection: SessionSelection): Promise<void>;
}
export interface KiteAcpOptions {
  machine: string;
  workspaces: string[];
  herdr: string;
  defaultProvider: ChatProvider;
  newSessionProviders?: readonly ChatProvider[];
  repository?: AcpRepository;
  stateDirectory?: string;
  placement?(): Promise<number | null>;
  humanAvailable?(): Promise<boolean>;
  inspect?(record: SessionRecord): Promise<OwnerStatus>;
  interrupt?(record: SessionRecord, turnId: string): Promise<void>;
  release?(record: SessionRecord): Promise<void>;
  backend?(
    record: SessionRecord,
    event: (message: RpcMessage) => void,
    request: (message: RpcMessage) => void,
  ): Promise<AcpBackend>;
}
const repository: AcpRepository = {
  list: async () => (await catalogRead()).sessions,
  configure: async (id, selection) => {
    await patchSession(id, { acp_selection: selection });
  },
  create: async (provider, cwd, title, delegation, selection) =>
    locked(join(catalogRoot(), "sessions.lock"), async () => {
      check(isChatProvider(provider), "network session provider is not supported");
      const catalog = catalogUnlocked(),
        at = Date.now();
      check(catalog.sessions.length < 1024, "session catalog is full");
      const parent = delegation?.parent;
      if (parent) {
        let children = 0;
        for (const r of catalog.sessions) {
          if (
            r.delegation?.parent?.machine === parent.machine &&
            r.delegation.parent.sessionId === parent.sessionId &&
            (!r.archived || (await ownerPresent(r).catch(() => true)))
          )
            children++;
          check(children < ACP_MAX_CHILDREN_PER_PARENT, "parent session child quota exceeded");
        }
      }
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
        ...(delegation ? { delegation } : {}),
        ...(selection ? { acp_selection: selection } : {}),
      };
      catalog.sessions.push(record);
      saveCatalogUnlocked(catalog);
      return record;
    }),
};

export function kiteAcpConnector(options: KiteAcpOptions) {
  const control = new AcpControl(options, options.repository ?? repository);
  return (send: (message: RpcMessage) => void) => new KiteAcp(options, send, control);
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
  private disconnect = new AbortController();
  private repository: AcpRepository;
  private sessionTail: Promise<unknown> = Promise.resolve();
  constructor(
    private options: KiteAcpOptions,
    private send: (message: RpcMessage) => void,
    private control = new AcpControl(options, options.repository ?? repository),
  ) {
    this.repository = options.repository ?? repository;
  }
  private cwd(path: string): string {
    return this.control.paths.cwd(path);
  }
  private supportsNewSession(provider: unknown): boolean {
    return (
      isChatProvider(provider) &&
      (this.options.newSessionProviders ?? CHAT_PROVIDERS).includes(provider)
    );
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
    binding.deltas.clear();
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
      const turn = object(params.turn);
      binding.backend.status =
        turn.status === "completed" || turn.status === "interrupted" ? "idle" : "systemError";
      this.publish(binding.record.id, {
        sessionUpdate: "session_info_update",
        updatedAt: new Date().toISOString(),
        _meta: { kite: { turnId: turn.id ?? null, status: turn.status ?? "unknown" } },
      });
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
    if (this.control.ownsTurn(binding.record.id) || (binding.record.delegation && !binding.turn))
      return;
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
        isChatProvider(record.provider) &&
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
            controlPlaneVersion: 2,
            controlMethods: this.options.placement
              ? [...ACP_CONTROL_METHODS, ACP_PLACEMENT_METHOD]
              : ACP_CONTROL_METHODS,
            newSessionProviders: [...(this.options.newSessionProviders ?? CHAT_PROVIDERS)],
            resumedSessionProviders: [...CHAT_PROVIDERS],
          },
        },
      };
    }
    check(this.initialized, "initialize the ACP connection first");
    if (method === "initialized") return {};
    if (method === ACP_PLACEMENT_METHOD) {
      check(this.options.placement, "peer resource placement is unavailable");
      check(
        this.supportsNewSession(params.provider),
        "network provider is not available on this host",
      );
      this.cwd(boundedString(params.cwd, 4096));
      return { score: await this.options.placement() };
    }
    if (method === "_kite/session/submit")
      return this.control.submit(params, this.disconnect.signal);
    if (method === "_kite/session/status")
      return this.control.status(acpSessionId(params.sessionId));
    if (method === "_kite/session/read") return this.control.read(params);
    if (method === "_kite/session/wait_any")
      return this.control.wait(params, this.disconnect.signal);
    if (method === "_kite/session/release") return this.control.release(params);
    if (method === "session/list") {
      if (optionalObject(params._meta).kite !== undefined) return this.control.list(params);
      const rows = (await this.repository.list()).filter((r) => {
        try {
          return isChatProvider(r.provider) && !r.archived && Boolean(this.cwd(r.cwd || ""));
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
          provider =
            meta.provider === undefined ? this.options.defaultProvider : string(meta.provider);
        check(
          meta.controlPlaneVersion === undefined || meta.controlPlaneVersion === 2,
          "unsupported Kite session control version",
        );
        if (method === "session/new")
          check(
            this.supportsNewSession(provider),
            "network provider is not available on this host",
          );
        if (
          method === "session/new" &&
          ["controlPlaneVersion", "delegation", "model", "effort", "serviceTier"].some(
            (key) => meta[key] !== undefined,
          )
        )
          return this.control.spawn(provider, cwd, meta);
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
              status: binding.backend.status || "unknown",
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
    if (method === "session/cancel" && !binding?.turn && !binding?.prompting)
      return this.control.cancel(acpSessionId(id));
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
    check(
      !binding.record.delegation?.writeLock &&
        (!binding.record.delegation || binding.record.delegation.policy === "strict"),
      "delegated session requires asynchronous submit for its write lock or permission policy",
    );
    check(!binding.turn && !binding.prompting, "session prompt is already active");
    const input = this.control.paths.prompt(params.prompt);
    binding.prompting = true;
    binding.cancelRequested = false;
    let unlock: (() => void) | undefined;
    try {
      unlock = await this.control.claimLegacy(binding.record);
      const meta = optionalObject(optionalObject(params._meta).kite);
      await verifyRevision(string(binding.record.cwd), meta.expectedRevision);
      const threadId = binding.backend.thread;
      if (binding.record.provider === "codex") {
        // Codex has live status before its first prompt creates turn history.
        const thread = object(
          object(await binding.backend.call("thread/read", { threadId, includeTurns: false }))
            .thread,
        );
        check(thread.id === threadId, "remote session thread changed");
        check(
          string(object(thread.status).type) !== "active",
          "remote session already has an active turn",
        );
      } else {
        const turns = array(
          object(
            await binding.backend.call("thread/turns/list", {
              threadId,
              limit: 1,
              itemsView: "notLoaded",
              sortDirection: "desc",
            }),
          ).data,
        ).map(object);
        check(turns[0]?.status !== "inProgress", "remote session already has an active turn");
      }
      if (binding.cancelRequested) return { stopReason: "cancelled" };
      const settings = acpTurnSettings(binding.record.provider, binding.backend.settings, "strict");
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
              input,
              ...(binding.record.provider === "codex" ? {} : { _kiteRequireIdle: true }),
              ...(settings ? { sessionSettings: settings } : {}),
              ...selectModel(
                binding.backend,
                binding.record.acp_selection ?? {},
                {},
                binding.record.provider === "codex",
              ),
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
      unlock?.();
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
    this.disconnect.abort();
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
