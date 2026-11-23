import { join } from "node:path";
import {
  array,
  boolean,
  boundedString,
  check,
  delegationPolicy,
  isChatProvider,
  type Json,
  json,
  type ObjectValue,
  object,
  optionalString,
  type RpcMessage,
  remoteSessionState,
  type SessionDelegation,
  type SessionRecord,
  type SessionSelection,
  sessionDelegation,
  sessionSelection,
  string,
} from "@kite/protocol";
import {
  ACP_MAX_WAIT_SESSIONS,
  ACP_READ_DEFAULT,
  ACP_READ_LIMIT,
  acpCommands,
  acpLimit,
  acpSessionId,
} from "@kite/protocol/acp";
import { home, readJson, sha256, sleep, tryLock } from "@kite/runtime";
import { listening, optionalObject, requirePrivateSocket } from "@kite/runtime/socket";
import type { AcpRepository, KiteAcpOptions } from "./acp";
import {
  type AcpBackend,
  connectBackend,
  selectModel,
  verifyRevision,
  WorkspacePaths,
  within,
} from "./acp-backend";
import { historyTurn } from "./acp-history";
import {
  gitWorkspace,
  inspectOwner,
  isActive,
  type OwnerStatus,
  requireAutoPermission,
  terminalState,
} from "./acp-owner";
import {
  acpStateDir,
  boundedTurn,
  type DelegatedInteraction,
  delegatedInteraction,
  recentTurns,
  saveTurn,
  type TurnResult,
  tailText,
  writeDelegatedInteraction,
} from "./acp-state";
import { backendRpc } from "./codex";
import { releaseRemoteSession } from "./lifecycle";

const MAX_ACTIVE_TURNS = 32;
const now = () => new Date().toISOString();
const stopReason = (status: string) =>
  status === "interrupted" ? "cancelled" : status === "failed" ? "failed" : "end_turn";
type SelectedModel = ReturnType<typeof selectModel>;
type Release = () => void;

function selectionForDisk(selected: SelectedModel): SessionSelection {
  return {
    model: selected.model,
    ...(selected.effort ? { effort: selected.effort } : {}),
    ...(selected.serviceTier ? { serviceTier: selected.serviceTier } : {}),
  };
}

interface Turn {
  record: SessionRecord;
  selected: SelectedModel;
  policy: SessionDelegation["policy"];
  commands: string[];
  backend?: AcpBackend;
  id?: string;
  updatedAt: string;
  unlock: Release;
  done: PromiseWithResolvers<TurnResult | undefined>;
  pending: Map<string | number, DelegatedInteraction>;
  messages: Map<string, string>;
  tools: Map<string, ObjectValue>;
  truncated: boolean;
  early: RpcMessage[];
  cancelRequested: boolean;
}

interface SessionStatus extends OwnerStatus, SessionSelection {
  sessionId: string;
  cwd: string;
  provider: string;
  updatedAt: string;
  revision: string;
  pendingInteraction?: DelegatedInteraction;
}

async function nodeAvailable(): Promise<boolean> {
  try {
    const config = object(readJson(join(home(), ".config/kite/node.json")));
    const socket = string(config.socket);
    requirePrivateSocket(socket);
    return await listening(socket);
  } catch {
    return false;
  }
}

/** A gateway retains a subscription to the existing owner independently of WebSocket clients. */
export class AcpControl {
  readonly paths: WorkspacePaths;
  private turns = new Map<string, Turn>();
  private waiters = new Set<() => void>();
  private workClaims = new Map<symbol, { record: SessionRecord; cwd: string; isolated: boolean }>();
  constructor(
    private options: KiteAcpOptions,
    private repo: AcpRepository,
  ) {
    this.paths = new WorkspacePaths(options.workspaces);
  }

  private directory() {
    return this.options.stateDirectory ?? acpStateDir();
  }
  ownsTurn(sessionId: string): boolean {
    return this.turns.has(sessionId);
  }

  private connect(
    record: SessionRecord,
    event: (message: RpcMessage) => void = () => {},
    request: (message: RpcMessage) => void = () => {},
  ) {
    return (
      this.options.backend?.(record, event, request) ??
      connectBackend(record, this.options.herdr, event, request)
    );
  }
  private inspect(record: SessionRecord) {
    return this.options.inspect?.(record) ?? inspectOwner(record);
  }
  private pending(turn: Turn): DelegatedInteraction | undefined {
    const values = [...turn.pending.values()];
    return values.find((p) => p.kind === "approval") ?? values[0];
  }
  private changed(turn?: Turn): void {
    if (turn) turn.updatedAt = now();
    for (const wake of this.waiters) wake();
  }
  private async record(id: string): Promise<SessionRecord> {
    const row = (await this.repo.list()).find((entry) => entry.id === id);
    check(row && this.visible(row), "session unavailable in configured workspaces");
    return row;
  }
  private visible(row: SessionRecord): boolean {
    try {
      return isChatProvider(row.provider) && !!this.paths.cwd(row.cwd ?? "");
    } catch {
      return false;
    }
  }
  private async ownerStatus(record: SessionRecord): Promise<OwnerStatus> {
    const turn = this.turns.get(record.id);
    if (!turn?.id) return this.inspect(record);
    const pending = this.pending(turn);
    return {
      state:
        pending?.kind === "approval"
          ? "waiting_approval"
          : pending?.kind === "question"
            ? "waiting_question"
            : "working",
      activeTurnId: turn.id,
    };
  }
  private async snapshot(record: SessionRecord): Promise<SessionStatus> {
    const owner = await this.ownerStatus(record);
    const turn = this.turns.get(record.id);
    const last = recentTurns(record, this.directory())[0];
    const pendingInteraction = isActive(owner.state)
      ? turn
        ? this.pending(turn)
        : delegatedInteraction(record, this.directory())
      : undefined;
    const select = turn?.id ? turn.selected : (last?.selection ?? record.acp_selection ?? {});
    const state = {
      sessionId: record.id,
      cwd: this.paths.cwd(string(record.cwd)),
      provider: record.provider,
      ...select,
      ...(last ? { lastTurnId: last.turnId, lastTurnStatus: last.lastTurnStatus } : {}),
      ...owner,
      ...(pendingInteraction ? { pendingInteraction } : {}),
      updatedAt:
        turn?.updatedAt ??
        last?.updatedAt ??
        new Date(Number(record.updated_ms) || 0).toISOString(),
    };
    const revision = sha256(JSON.stringify(state));
    return { ...state, revision };
  }
  async status(id: string): Promise<Json> {
    const record = await this.record(id);
    const [status, git] = await Promise.all([
      this.snapshot(record),
      gitWorkspace(this.paths.cwd(string(record.cwd))).catch(() => undefined),
    ]);
    return json({
      ...status,
      writeLock: record.delegation?.writeLock ?? false,
      ...(git ? { gitBranch: git.branch, worktree: git.path } : {}),
    });
  }

  private sessionLease(id: string): Release {
    acpSessionId(id);
    const release = tryLock(join(this.directory(), `session-${id}.lock`));
    check(release, "remote session has a submission or release in progress");
    return release;
  }

  async claimLegacy(record: SessionRecord): Promise<Release> {
    const unlock = this.sessionLease(record.id);
    try {
      const releaseWork = await this.workLease(record, "strict");
      return () => {
        releaseWork();
        unlock();
      };
    } catch (error) {
      unlock();
      throw error;
    }
  }

  private async workLease(
    record: SessionRecord,
    policy: SessionDelegation["policy"],
  ): Promise<Release> {
    const isolated = policy === "isolated_auto";
    const cwd = this.paths.cwd(string(record.cwd));
    if (policy === "human_in_the_loop") {
      check(
        await (this.options.humanAvailable?.() ?? nodeAvailable()),
        "human intervention requires a reachable Kite host",
      );
    }
    if (isolated || record.delegation?.worktreeIsolated) {
      check(
        record.delegation?.worktreeIsolated && (await gitWorkspace(cwd, true))?.linked,
        "a separate linked Git worktree is required",
      );
    }
    const conflict = (other: SessionRecord, path: string, otherIsolated: boolean) =>
      ((isolated || otherIsolated) && (within(cwd, path) || within(path, cwd))) ||
      (path === cwd &&
        record.delegation?.writeLock === true &&
        other.delegation?.writeLock === true);
    for (const claim of this.workClaims.values())
      if (conflict(claim.record, claim.cwd, claim.isolated)) {
        const state = this.turns.get(claim.record.id);
        const detail = state?.id ? "active turn" : "submission in progress";
        throw new Error(
          `workspace is occupied by session ${claim.record.id || "(creating)"} (${claim.record.provider}, ${detail})`,
        );
      }
    const unlock =
      isolated || record.delegation?.writeLock
        ? tryLock(join(this.directory(), `cwd-${sha256(cwd)}.lock`))
        : () => {};
    check(unlock, "workspace write lock is held by another gateway");
    const claim = Symbol();
    this.workClaims.set(claim, { record, cwd, isolated });
    const release = () => {
      this.workClaims.delete(claim);
      unlock();
    };
    try {
      for (const row of await this.repo.list()) {
        if (row.id === record.id || !row.cwd) continue;
        let other: string;
        try {
          other = this.paths.cwd(row.cwd);
        } catch {
          continue;
        }
        const otherIsolated =
          row.delegation?.worktreeIsolated === true &&
          (row.delegation.policy === "isolated_auto" ||
            this.turns.get(row.id)?.policy === "isolated_auto");
        if (conflict(row, other, otherIsolated)) {
          check(isChatProvider(row.provider), "overlapping session owner cannot be verified");
          const state = await this.ownerStatus(row);
          check(
            !isActive(state.state) && !this.turns.has(row.id),
            `workspace is occupied by session ${row.id} (${row.provider}, ${state.state})`,
          );
        }
      }
      return release;
    } catch (error) {
      release();
      throw error;
    }
  }

  async spawn(provider: string, cwd: string, meta: ObjectValue): Promise<Json> {
    const defaults = sessionSelection(meta);
    const delegation =
      meta.delegation === undefined
        ? { policy: "strict" as const }
        : sessionDelegation(meta.delegation);
    const title =
      delegation.label ?? (meta.title === undefined ? "" : boundedString(meta.title, 512));
    const release = await this.workLease(
      { id: "", provider: this.options.defaultProvider, cwd, title, delegation },
      delegation.policy,
    );
    let record: SessionRecord | undefined;
    let backend: AcpBackend | undefined;
    try {
      record = await this.repo.create(provider, cwd, title, delegation, defaults);
      backend = await this.connect(record);
      const selected = selectModel(backend, defaults, {}, provider === "codex");
      if (delegation.policy === "isolated_auto") requireAutoPermission(provider, backend.settings);
      await this.repo.configure?.(record.id, selectionForDisk(selected));
      return {
        sessionId: record.id,
        _meta: {
          kite: {
            machine: this.options.machine,
            provider,
            threadId: backend.thread,
            state: "idle",
            ...selected,
            delegation: json(delegation),
          },
        },
      };
    } catch (error) {
      if (!record) throw error;
      if (backend) {
        backend.close();
        backend = undefined;
        try {
          await this.reclaim(record);
        } catch {
          // Keep a discoverable record when owner startup or release cannot be verified.
        }
      }
      throw new Error(
        `${error instanceof Error ? error.message : "session creation failed"}; remote session: ${record.id}`,
      );
    } finally {
      backend?.close();
      release();
    }
  }

  async list(params: ObjectValue): Promise<Json> {
    const filter = optionalObject(optionalObject(params._meta).kite);
    const parentId =
      filter.parentSessionId === undefined ? undefined : boundedString(filter.parentSessionId, 128);
    const parentMachine =
      filter.parentMachine === undefined ? undefined : boundedString(filter.parentMachine, 128);
    const label = filter.label === undefined ? undefined : boundedString(filter.label, 120);
    const includeArchived =
      filter.includeArchived === undefined ? false : boolean(filter.includeArchived);
    const states = filter.states === undefined ? [] : array(filter.states).map(remoteSessionState);
    check(states.length <= 6, "too many session states");
    const rows = (await this.repo.list()).filter(
      (r) =>
        this.visible(r) &&
        (includeArchived || !r.archived) &&
        (!parentId || r.delegation?.parent?.sessionId === parentId) &&
        (!parentMachine || r.delegation?.parent?.machine === parentMachine) &&
        (!label || r.delegation?.label === label),
    );
    const cursor = optionalString(params.cursor) || "0";
    check(/^\d{1,6}$/.test(cursor) && Number(cursor) <= rows.length, "invalid session cursor");
    const start = Number(cursor),
      end = Math.min(start + 32, rows.length);
    const sessions: Json[] = [];
    for (let offset = start; offset < end; offset += 8) {
      const page = await Promise.all(
        rows.slice(offset, Math.min(offset + 8, end)).map(async (row) => {
          const cwd = this.paths.cwd(string(row.cwd));
          const [status, git] = await Promise.all([
            this.snapshot(row).catch(() => undefined),
            gitWorkspace(cwd).catch(() => undefined),
          ]);
          if (states.length && (!status || !states.includes(status.state))) return;
          return {
            sessionId: row.id,
            cwd,
            title: row.title,
            updatedAt: status?.updatedAt ?? new Date(Number(row.updated_ms) || 0).toISOString(),
            _meta: {
              kite: {
                machine: this.options.machine,
                provider: row.provider,
                archived: row.archived === true,
                ...(status
                  ? {
                      state: status.state,
                      activeTurnId: status.activeTurnId,
                      revision: status.revision,
                    }
                  : {}),
                ...(row.delegation
                  ? {
                      parent: json(row.delegation.parent ?? null),
                      label: row.delegation.label,
                      role: row.delegation.role,
                      writeLock: row.delegation.writeLock,
                    }
                  : {}),
                ...(git ? { gitBranch: git.branch, worktree: git.path } : {}),
              },
            },
          };
        }),
      );
      for (const result of page) if (result) sessions.push(result);
    }
    return { sessions, ...(end < rows.length ? { nextCursor: String(end) } : {}) };
  }

  private updateInteraction(turn: Turn): void {
    if (turn.policy === "human_in_the_loop")
      writeDelegatedInteraction(turn.record, this.pending(turn), this.directory());
    this.changed(turn);
  }

  private interaction(turn: Turn, message: RpcMessage): void {
    const params = optionalObject(message.params);
    const id = message.id;
    if (typeof id !== "string" && typeof id !== "number") return;
    if (
      !message.method ||
      !turn.backend ||
      params.threadId !== turn.backend.thread ||
      (params.turnId !== undefined && params.turnId !== turn.id)
    )
      return;
    const approval = message.method.endsWith("requestApproval");
    const question = message.method === "item/tool/requestUserInput";
    if (turn.policy === "human_in_the_loop" && (approval || question)) {
      const pending: DelegatedInteraction = {
        kind: approval ? "approval" : "question",
        id: tailText(String(id), 128),
        turnId: turn.id,
        ...(typeof params.command === "string"
          ? { command: tailText(params.command, 16 * 1024) }
          : {}),
        ...(typeof params.cwd === "string" ? { cwd: tailText(params.cwd, 4096) } : {}),
        ...(question && Array.isArray(params.questions)
          ? {
              question: tailText(
                params.questions.map((q) => optionalString(optionalObject(q).question)).join("\n"),
                4096,
              ),
            }
          : {}),
      };
      if (turn.pending.size < 32) turn.pending.set(id, pending);
      this.updateInteraction(turn);
      return;
    }
    if (approval) {
      const rawCwd =
        params.toolInput === undefined ? params.cwd : optionalObject(params.toolInput).cwd;
      const accepted =
        turn.policy === "strict" &&
        message.method === "item/commandExecution/requestApproval" &&
        rawCwd === turn.record.cwd &&
        typeof params.command === "string" &&
        turn.commands.includes(params.command) &&
        !turn.cancelRequested;
      turn.backend.write({ id, result: { decision: accepted ? "accept" : "decline" } });
    } else {
      turn.backend.write({
        id,
        error: { code: -32601, message: "Interaction requires a Kite client" },
      });
    }
  }

  private event(turn: Turn, message: RpcMessage): void {
    const params = optionalObject(message.params);
    if (turn.backend && params.threadId !== turn.backend.thread) return;
    if (message.method === "_kite/backend_closed") {
      this.complete(turn);
      return;
    }
    if (!turn.id || !turn.backend) {
      if (turn.early.length < 32 && Buffer.byteLength(JSON.stringify(message)) <= 128 * 1024) {
        turn.early.push(message);
      } else {
        turn.truncated = true;
      }
      return;
    }
    if (message.id !== undefined) {
      this.interaction(turn, message);
      return;
    }
    if (params.turnId !== undefined && params.turnId !== turn.id) return;
    if (message.method === "turn/completed") {
      const completed = object(params.turn);
      if (completed.id === turn.id) this.complete(turn, completed);
    } else if (message.method === "serverRequest/resolved") {
      if (
        (typeof params.requestId === "string" || typeof params.requestId === "number") &&
        turn.pending.delete(params.requestId)
      )
        this.updateInteraction(turn);
    } else if (
      message.method === "item/agentMessage/delta" ||
      message.method === "item/completed" ||
      message.method === "item/started"
    ) {
      const item = optionalObject(params.item);
      const id = optionalString(params.itemId) || optionalString(item.id);
      if (id) this.project(turn, id, item, message.method, optionalString(params.delta));
    }
  }

  private project(turn: Turn, id: string, item: ObjectValue, method: string, delta: string) {
    if (method === "item/agentMessage/delta" || item.type === "agentMessage") {
      const previous = turn.messages.get(id) ?? "";
      const text =
        method === "item/agentMessage/delta" ? previous + delta : optionalString(item.text);
      const reply = tailText(text, ACP_READ_LIMIT);
      turn.truncated ||= text !== reply;
      turn.messages.set(id, reply);
      let size = 0;
      for (const [key, value] of [...turn.messages].reverse()) {
        size += Buffer.byteLength(value);
        if (size > 2 * ACP_READ_LIMIT || turn.messages.size > 64) {
          turn.messages.delete(key);
          turn.truncated = true;
        }
      }
    } else if (
      ["commandExecution", "fileChange", "mcpToolCall", "dynamicToolCall", "webSearch"].includes(
        optionalString(item.type),
      )
    ) {
      const previous = turn.tools.get(id);
      const output = optionalString(item.aggregatedOutput) || optionalString(item.output);
      const retained = tailText(output, 32 * 1024);
      turn.truncated ||= retained !== output || optionalObject(item._meta).outputTruncated === true;
      turn.tools.set(id, {
        id: tailText(id, 128),
        type: item.type,
        command: tailText(
          optionalString(item.command) ||
            optionalString(item.name) ||
            optionalString(previous?.command),
          2048,
        ),
        status:
          item.status === "declined"
            ? "declined"
            : item.status === "failed" ||
                item.failed === true ||
                (typeof item.exitCode === "number" && item.exitCode !== 0)
              ? "failed"
              : method === "item/started"
                ? "in_progress"
                : "completed",
        ...(typeof item.exitCode === "number" ? { exitCode: item.exitCode } : {}),
        output: retained,
      });
      if (turn.tools.size > 24) {
        const oldest = turn.tools.keys().next().value;
        if (oldest) turn.tools.delete(oldest);
        turn.truncated = true;
      }
    }
  }

  private complete(turn: Turn, completed?: ObjectValue): void {
    if (this.turns.get(turn.record.id) !== turn) return;
    let result: TurnResult | undefined;
    try {
      if (completed && turn.id) {
        const status = string(completed.status);
        result = {
          turnId: turn.id,
          reply: [...turn.messages.values()].join(""),
          tools: [...turn.tools.values()],
          stopReason: stopReason(status),
          lastTurnStatus: status,
          updatedAt: now(),
          truncated: turn.truncated,
          selection: selectionForDisk(turn.selected),
        };
        saveTurn(turn.record, result, this.options.stateDirectory);
      }
    } catch {
      // The owner history remains available even when the projection cannot be written.
      result = undefined;
    } finally {
      this.turns.delete(turn.record.id);
      turn.backend?.close();
      try {
        if (turn.policy === "human_in_the_loop")
          writeDelegatedInteraction(turn.record, undefined, this.directory());
      } finally {
        turn.unlock();
        turn.done.resolve(result);
        this.changed();
      }
    }
  }

  async submit(params: ObjectValue, signal: AbortSignal): Promise<Json> {
    const id = acpSessionId(params.sessionId);
    const requested = sessionSelection(params);
    const input = this.paths.prompt(params.prompt);
    const commands = acpCommands(params.allowCommands);
    const waitMs = acpLimit(params.waitMs, 0, 30_000);
    check(this.turns.size < MAX_ACTIVE_TURNS, "gateway active session limit reached");
    const unlock = this.sessionLease(id);
    let work: Release | undefined;
    const releaseWork = () => {
      work?.();
      work = undefined;
    };
    let turn: Turn | undefined;
    let handedOff = false;
    try {
      const record = await this.record(id);
      check(!record.archived, "remote session is archived");
      const policy =
        params.policy === undefined
          ? (record.delegation?.policy ?? "strict")
          : delegationPolicy(params.policy);
      const cwd = this.paths.cwd(string(record.cwd));
      work = await this.workLease(record, policy);
      await verifyRevision(cwd, params.expectedRevision);
      const status = await this.ownerStatus(record);
      check(!isActive(status.state), "remote session already has an active turn");
      check(this.turns.size < MAX_ACTIVE_TURNS, "gateway active session limit reached");
      const current: Turn = {
        record,
        policy,
        commands,
        cancelRequested: false,
        updatedAt: now(),
        selected: { model: "", effort: "" },
        done: Promise.withResolvers(),
        unlock: () => {
          releaseWork();
          unlock();
        },
        pending: new Map(),
        messages: new Map(),
        tools: new Map(),
        truncated: false,
        early: [],
      };
      turn = current;
      this.turns.set(id, turn);
      const backend = await this.connect(
        record,
        (message) => this.event(current, message),
        (message) => this.event(current, message),
      );
      if (this.turns.get(id) !== current) {
        backend.close();
        throw new Error("owner disconnected during attachment");
      }
      current.backend = backend;
      check(backend.status !== "active", "remote session already has an active turn");
      current.selected = selectModel(
        backend,
        record.acp_selection ?? {},
        requested,
        record.provider === "codex",
      );
      const settings =
        policy === "isolated_auto"
          ? requireAutoPermission(record.provider, backend.settings)
          : undefined;
      check(!signal.aborted, "connection closed before submitting");
      check(!current.cancelRequested, "submission cancelled before delivery");
      // Serialize against the actual owner, including clients outside the gateway.
      const ack = object(
        object(
          await backend.call("turn/start", {
            threadId: backend.thread,
            input,
            ...current.selected,
            ...(record.provider === "codex" ? {} : { _kiteRequireIdle: true }),
            ...(settings ? { sessionSettings: settings } : {}),
          }),
        ).turn,
      );
      current.id = boundedString(ack.id, 128);
      check(
        this.turns.get(id) === current,
        "owner disconnected before acknowledgement; inspect the remote session",
      );
      handedOff = true;
      for (const message of current.early.splice(0)) {
        if (this.turns.get(id) !== current) break;
        this.event(current, message);
      }
      if (
        this.turns.get(id) === current &&
        ["completed", "interrupted", "failed"].includes(optionalString(ack.status))
      )
        this.complete(current, ack);
      this.changed(current);
      const turnId = current.id;
      if (current.cancelRequested && this.turns.get(id) === current) {
        await backend.call("turn/interrupt", { threadId: backend.thread, turnId });
      }
      const result = waitMs ? await this.awaitResult(current, waitMs, signal) : undefined;
      if (result)
        return {
          ...boundedTurn(id, result, ACP_READ_DEFAULT, false),
          state: terminalState(result.lastTurnStatus),
          completed: true,
        };
      return {
        sessionId: id,
        turnId,
        state:
          this.pending(current)?.kind === "approval"
            ? "waiting_approval"
            : this.pending(current)?.kind === "question"
              ? "waiting_question"
              : "working",
      };
    } finally {
      if (!handedOff) {
        if (turn && this.turns.get(id) === turn) this.complete(turn);
        else if (!turn) {
          releaseWork();
          unlock();
        }
      }
    }
  }

  private async awaitResult(turn: Turn, timeout: number, signal: AbortSignal) {
    const timeoutResult = Promise.withResolvers<undefined>();
    const abort = () => timeoutResult.resolve(undefined);
    const timer = setTimeout(abort, timeout);
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
    try {
      return await Promise.race([turn.done.promise, timeoutResult.promise]);
    } finally {
      clearTimeout(timer);
      signal.removeEventListener("abort", abort);
    }
  }

  async read(params: ObjectValue): Promise<Json> {
    const record = await this.record(acpSessionId(params.sessionId));
    const turnId = params.turnId === undefined ? undefined : boundedString(params.turnId, 128);
    const max = acpLimit(params.maxBytes, ACP_READ_DEFAULT, ACP_READ_LIMIT, 1);
    const includeTools =
      params.includeToolSummaries === undefined ? true : boolean(params.includeToolSummaries);
    const turns = recentTurns(record, this.directory());
    if (turnId) {
      const saved = turns.find((result) => result.turnId === turnId) ?? historyTurn(record, turnId);
      if (saved) return boundedTurn(record.id, saved, max, includeTools);
    }
    const status = await this.snapshot(record);
    if (!turnId && !status.activeTurnId) {
      const saved =
        turns.find((result) => result.turnId === status.lastTurnId) ??
        historyTurn(record, status.lastTurnId) ??
        turns[0];
      if (saved) return boundedTurn(record.id, saved, max, includeTools);
    }
    check(!turnId || status.activeTurnId === turnId, "turn result is unavailable");
    return {
      sessionId: record.id,
      turnId: status.activeTurnId,
      state: status.state,
      completed: false,
    };
  }

  async wait(params: ObjectValue, signal: AbortSignal): Promise<Json> {
    const ids = array(params.sessionIds).map(acpSessionId);
    check(
      ids.length > 0 && ids.length <= ACP_MAX_WAIT_SESSIONS && new Set(ids).size === ids.length,
      "invalid wait session list",
    );
    const since = params.since === undefined ? {} : object(params.since);
    for (const [id, revision] of Object.entries(since)) {
      check(ids.includes(id), "unknown wait session revision");
      boundedString(revision, 128);
    }
    const deadline = Date.now() + acpLimit(params.timeoutMs, 60_000, 60_000);
    while (!signal.aborted) {
      const changed = Promise.withResolvers<void>();
      const wake = () => changed.resolve();
      this.waiters.add(wake);
      signal.addEventListener("abort", wake, { once: true });
      let timer: ReturnType<typeof setTimeout> | undefined;
      try {
        const records = await this.repo.list();
        const sessions = await Promise.all(
          ids.map((id) => {
            const record = records.find((row) => row.id === id);
            check(
              record && this.visible(record),
              "wait session unavailable in configured workspaces",
            );
            return this.snapshot(record);
          }),
        );
        const ready = sessions.find(
          (session) => session.state !== "working" && since[session.sessionId] !== session.revision,
        );
        if (ready) return json({ timedOut: false, sessionId: ready.sessionId, sessions });
        const remaining = deadline - Date.now();
        if (remaining <= 0) return json({ timedOut: true, sessions });
        timer = setTimeout(wake, Math.min(750, remaining));
        await changed.promise;
      } finally {
        clearTimeout(timer);
        this.waiters.delete(wake);
        signal.removeEventListener("abort", wake);
      }
    }
    throw new Error("connection closed while waiting");
  }

  async cancel(id: string): Promise<Json> {
    const record = await this.record(id);
    const turn = this.turns.get(id);
    if (turn && !turn.id) {
      turn.cancelRequested = true;
      return { sessionId: id, cancellationRequested: true };
    }
    const status = await this.ownerStatus(record);
    if (!isActive(status.state)) return json(await this.snapshot(record));
    const turnId = status.activeTurnId;
    check(turnId, "active turn identity unavailable");
    try {
      const backend = turn?.backend;
      if (turn) turn.cancelRequested = true;
      if (backend) await backend.call("turn/interrupt", { threadId: backend.thread, turnId });
      else if (this.options.interrupt) await this.options.interrupt(record, turnId);
      else {
        check(record.pane_id && record.reference, "active owner identity unavailable");
        await backendRpc(record.pane_id, "turn/interrupt", {
          threadId: record.reference.value,
          turnId,
        });
      }
    } catch (error) {
      if ((await this.ownerStatus(record)).activeTurnId === turnId) throw error;
    }
    return json(await this.snapshot(record));
  }

  private reclaim(record: SessionRecord) {
    return this.options.release?.(record) ?? releaseRemoteSession(record.id, this.options.herdr);
  }

  async release(params: ObjectValue): Promise<Json> {
    const id = acpSessionId(params.sessionId);
    const force = params.forceCancel === undefined ? false : boolean(params.forceCancel);
    let record = await this.record(id);
    let status = await this.ownerStatus(record);
    if (status.state === "released")
      return json({ ...(await this.snapshot(record)), released: true });
    if (isActive(status.state) || this.turns.has(id)) {
      if (!force)
        return {
          ...object(json(await this.snapshot(record))),
          released: false,
          reason: "active_turn",
        };
      await this.cancel(id);
      const deadline = Date.now() + 10_000;
      do {
        if (Date.now() >= deadline)
          return {
            ...object(json(await this.snapshot(record))),
            released: false,
            reason: "cancellation_pending",
          };
        await sleep(100);
        record = await this.record(id);
        status = await this.ownerStatus(record);
      } while (isActive(status.state) || this.turns.has(id));
    }
    const unlock = this.sessionLease(id);
    try {
      await this.reclaim(await this.record(id));
      return json({ ...(await this.snapshot(await this.record(id))), released: true });
    } finally {
      unlock();
    }
  }
}
