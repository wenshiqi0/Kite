import { existsSync, readFileSync, statSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import { killScope, type OwnerIdentity, reclaimCache, scopeName, stopOwner } from "@kite/platform";
import {
  array,
  check,
  type Json,
  json,
  type ObjectValue,
  object,
  type Provider,
  RPC_LIMIT,
  type SessionRecord,
  string,
  UUID,
} from "@kite/protocol";
import {
  home,
  live,
  locked,
  pgid,
  processStartedMs,
  readJson,
  same,
  sha256,
  sleep,
  startTicks,
  stopped,
} from "@kite/runtime";
import { catalogRead, catalogRoot, catalogUnlocked, sessionLock } from "./catalog";
import { api, backendRpc, ownerLock, readRegistry, registryPath, sameReference } from "./codex";
import { harnessAgents, harnessDirectory, harnessInfo } from "./harness";
import { historyFiles, questionRecords } from "./history";

export interface LifecycleFact {
  session_id: string;
  pane: string;
  owner: OwnerIdentity;
  reference: SessionRecord["reference"];
  archived: boolean;
  idle: boolean;
  persisted: boolean;
  activity: string;
  last_activity_ms: number;
  tui_pid: number | null;
}
async function codexLifecycleFact(
  record: SessionRecord,
  binary: string,
  knownAgents?: ObjectValue[],
): Promise<LifecycleFact> {
  const pane = record.pane_id;
  check(pane, "missing pane");
  const owner = readRegistry(pane),
    agents = knownAgents ?? array((await api(binary, "agent.list", {})).agents).map(object),
    agent = agents.find((a) => a.pane_id === pane),
    p = object((await api(binary, "pane.process_info", { pane_id: pane })).process_info),
    front = array(p.foreground_processes).map(object),
    shell = p.foreground_process_group_id === p.shell_pid,
    first = front[0],
    remote =
      front.length === 1 &&
      first?.name === "codex" &&
      array(first.argv).includes("--remote") &&
      array(first.argv).includes(`unix://${owner.socket}`),
    idle =
      (!agent && shell) ||
      Boolean(
        agent &&
          sameReference(agent, record) &&
          ["idle", "done"].includes(String(agent.agent_status)) &&
          agent.interactive_ready &&
          remote &&
          !stopped(Number(first?.pid)),
      );
  const loaded = array(object(await backendRpc(pane, "thread/loaded/list", {}, 2000)).data).map(
    string,
  );
  check(loaded.length <= 64, "loaded thread capacity exceeded");
  const statuses: string[] = [],
    histories: Json[] = [],
    mtimes: number[] = [],
    root = join(process.env.CODEX_HOME || join(home(), ".codex"), "sessions");
  const inspect = (thread: string) => {
    const files = historyFiles(root, thread);
    if (files.length === 1 && files[0]) {
      const s = statSync(files[0], { bigint: true });
      histories.push([thread, Number(s.dev), Number(s.ino), Number(s.size), Number(s.mtimeNs)]);
      mtimes.push(Number(s.mtimeNs / 1000000n));
    }
  };
  for (const thread of loaded) {
    check(UUID.test(thread), "invalid loaded thread identity");
    const value = object(
      object(await backendRpc(pane, "thread/read", { threadId: thread, includeTurns: false }, 2000))
        .thread,
    );
    check(value.id === thread, "loaded thread identity changed");
    statuses.push(String(object(value.status).type));
    inspect(thread);
  }
  if (!loaded.length && record.reference?.kind === "id" && UUID.test(record.reference.value))
    inspect(record.reference.value);
  const startedMs = processStartedMs(owner.start_ticks),
    lastOpen = owner.last_open_ms ?? startedMs;
  return {
    session_id: record.id,
    pane,
    owner,
    reference: record.reference ?? null,
    archived: Boolean(record.archived),
    idle:
      idle &&
      statuses.every((s) => s === "idle") &&
      (!loaded.length || loaded.includes(record.reference?.value || "")),
    persisted: histories.length >= loaded.length,
    activity: sha256(JSON.stringify([lastOpen, histories.sort()])),
    last_activity_ms: Math.max(lastOpen, ...mtimes),
    tui_pid: remote ? Number(first?.pid) : null,
  };
}
/** Every managed provider proves its own durable context and owns its release mechanism. */
export interface HarnessLifecycle {
  snapshot(record: SessionRecord, agents?: ObjectValue[]): Promise<LifecycleFact>;
  ownerLock(record: SessionRecord): string;
  sharesOwner(record: SessionRecord, owner: OwnerIdentity, other: SessionRecord): boolean;
  release(record: SessionRecord, expected: LifecycleFact): Promise<Json>;
}
export async function managedLifecycleFact(record: SessionRecord): Promise<LifecycleFact> {
  const ref = record.reference;
  check(
    (record.provider === "dsh" || record.provider === "ply") &&
      ref?.source === "kite:harness" &&
      ref.agent === record.provider &&
      ref.kind === "id",
    "unmanaged harness reference",
  );
  const path = harnessDirectory(ref.value),
    info = harnessInfo(readJson(join(path, "session.json"))),
    state = object(readJson(join(path, "state.json"))),
    pid = Number(state.pid),
    ticks = string(state.start_ticks);
  check(
    info.id === record.id &&
      info.thread === ref.value &&
      info.provider === record.provider &&
      startTicks(pid) === ticks,
    "harness owner identity changed",
  );
  const pane = record.pane_id;
  check(pane, "missing harness pane");
  const owner: OwnerIdentity = {
      pane,
      pid,
      start_ticks: ticks,
      ...(process.platform === "linux" ? { budget_unit: scopeName(pane, pid, ticks) } : {}),
    },
    histories: Json[] = [],
    mtimes: number[] = [];
  for (const name of ["history.jsonl", "native.json", "queued.json"]) {
    const file = join(path, name);
    if (existsSync(file)) {
      const stat = statSync(file, { bigint: true });
      histories.push([
        name,
        Number(stat.dev),
        Number(stat.ino),
        Number(stat.size),
        Number(stat.mtimeNs),
      ]);
      mtimes.push(Number(stat.mtimeNs / 1000000n));
    }
  }
  const queued = join(path, "queued.json"),
    context = join(path, "native.json"),
    queueEmpty = !existsSync(queued) || same(readJson(queued, 36 * 1024 * 1024), []);
  const saved = existsSync(context) ? readJson(context, RPC_LIMIT) : null;
  const hasContext = Array.isArray(saved)
    ? saved.length > 0
    : saved && typeof saved === "object"
      ? Object.keys(saved).length > 0
      : Boolean(saved);
  const restorable = existsSync(context)
    ? hasContext
    : state.status === "idle" && !existsSync(join(path, "history.jsonl"));
  return {
    session_id: record.id,
    pane,
    owner,
    reference: ref,
    archived: Boolean(record.archived),
    idle:
      state.protocol === 2 &&
      ["idle", "done", "failed", "cancelled"].includes(String(state.status)) &&
      queueEmpty,
    persisted: restorable && queueEmpty,
    activity: sha256(JSON.stringify([state, histories.sort()])),
    last_activity_ms: Math.max(0, ...mtimes),
    tui_pid: null,
  };
}
const changed = (a: LifecycleFact, b: LifecycleFact) =>
  ["owner", "reference", "activity"].some((k) => !same(object(json(a))[k], object(json(b))[k]));
async function releaseCodex(
  record: SessionRecord,
  fresh: LifecycleFact,
  binary: string,
): Promise<Json> {
  const owner = fresh.owner,
    pane = fresh.pane,
    no = (reason: string) => ({ reclaimed: false, reason });
  if (fresh.tui_pid !== null) {
    const pid = fresh.tui_pid,
      ticks = startTicks(pid);
    check(ticks, "TUI identity unavailable");
    if (stopped(pid)) return no("tui_paused");
    process.kill(pid, "SIGTERM");
    const deadline = Date.now() + 4000;
    while (live(pid, ticks) && Date.now() < deadline) await sleep(50);
    if (live(pid, ticks)) return no("tui_still_connected");
  }
  const after = await codexLifecycleFact(record, binary);
  if (changed(after, fresh) || !after.idle || !after.persisted) return no("activity_changed");
  if (!same(readRegistry(pane), owner) || pgid(owner.pid) !== owner.pid) return no("owner_changed");
  if (owner.budget_unit) await stopOwner(owner);
  else process.kill(-owner.pid, "SIGTERM");
  const deadline = Date.now() + 4000;
  while (live(owner.pid, owner.start_ticks) && Date.now() < deadline) await sleep(50);
  if (live(owner.pid, owner.start_ticks)) return no("owner_still_running");
  if (same(readJson(registryPath(pane)), owner)) unlinkSync(registryPath(pane));
  return { reclaimed: true, session_id: record.id };
}
export function lifecycleAdapters(binary: string): Partial<Record<Provider, HarnessLifecycle>> {
  const managed: HarnessLifecycle = {
    snapshot: managedLifecycleFact,
    ownerLock: (record) => {
      check(record.reference, "missing harness reference");
      return join(harnessDirectory(record.reference.value), "owner.lock");
    },
    sharesOwner: (record, _owner, other) => same(other.reference, record.reference),
    release: async (record, expected) => {
      const fresh = await managedLifecycleFact(record);
      if (changed(fresh, expected) || !fresh.idle || !fresh.persisted)
        return { reclaimed: false, reason: "activity_changed" };
      const owner = fresh.owner;
      check(startTicks(owner.pid) === owner.start_ticks, "harness owner changed before release");
      if (process.platform === "darwin") {
        check(pgid(owner.pid) === owner.pid, "harness process group identity changed");
        process.kill(-owner.pid, "SIGTERM");
        const deadline = Date.now() + 4000;
        while (live(owner.pid, owner.start_ticks) && Date.now() < deadline) await sleep(50);
        return { reclaimed: !live(owner.pid, owner.start_ticks), session_id: record.id };
      }
      const cgroup = readFileSync(`/proc/${owner.pid}/cgroup`, "utf8").trim();
      check(
        owner.budget_unit && cgroup.endsWith(`/kite-agents.slice/${owner.budget_unit}`),
        "harness owner is outside its managed scope",
      );
      await reclaimCache(owner.budget_unit);
      await killScope(owner.budget_unit);
      return { reclaimed: true, session_id: record.id };
    },
  };
  return {
    codex: {
      snapshot: (record, agents) => codexLifecycleFact(record, binary, agents),
      ownerLock: (record) => {
        check(record.pane_id, "missing pane");
        return ownerLock(record.pane_id);
      },
      sharesOwner: (_record, owner, other) => {
        if (!other.pane_id) return false;
        try {
          const alias = readRegistry(other.pane_id);
          return alias.pid === owner.pid && alias.start_ticks === owner.start_ticks;
        } catch {
          return false;
        }
      },
      release: (record, expected) => releaseCodex(record, expected, binary),
    },
    dsh: managed,
    ply: managed,
  };
}
export async function lifecycleFact(
  record: SessionRecord,
  binary: string,
  agents?: ObjectValue[],
): Promise<LifecycleFact> {
  const adapter = lifecycleAdapters(binary)[record.provider];
  check(adapter, "unsupported lifecycle provider");
  return adapter.snapshot(record, agents);
}
export async function lifecycleFacts(binary: string): Promise<Json> {
  const records = (await catalogRead()).sessions,
    adapters = lifecycleAdapters(binary),
    facts: LifecycleFact[] = [];
  let agents: ObjectValue[] = [];
  try {
    agents = array((await api(binary, "agent.list", {})).agents).map(object);
  } catch {
    /* managed harnesses do not depend on Herdr availability */
  }
  for (const record of records) {
    const adapter = adapters[record.provider];
    if (!adapter || !record.pane_id) continue;
    try {
      facts.push(await adapter.snapshot(record, agents));
    } catch {
      /* unavailable facts never authorize stopping */
    }
  }
  return json({ facts });
}
export async function lifecycleReap(
  expected: ObjectValue,
  binary: string,
  adapters = lifecycleAdapters(binary),
): Promise<Json> {
  const id = string(expected.session_id),
    pane = string(expected.pane);
  check(expected.reason === "archived", "invalid lifecycle request");
  const no = (reason: string) => ({ reclaimed: false, reason });
  return locked(sessionLock(id), async () => {
    const record = (await catalogRead()).sessions.find((r) => r.id === id),
      adapter = record ? adapters[record.provider] : undefined;
    if (!record || !adapter || record.pane_id !== pane) return no("record_changed");
    return locked(adapter.ownerLock(record), async () => {
      if (!record.archived) return no("unarchived");
      const fresh = await adapter.snapshot(record);
      for (const field of ["owner", "reference", "activity", "archived"] as const)
        if (!same(fresh[field], expected[field])) return no("activity_changed");
      if (!fresh.idle || !fresh.persisted) return no("not_idle_or_not_persisted");
      return locked(join(catalogRoot(), "sessions.lock"), async () => {
        const latest = catalogUnlocked().sessions,
          current = latest.find((r) => r.id === id);
        if (
          !current ||
          ["archived", "reference", "provider", "pane_id"].some((k) => !same(current[k], record[k]))
        )
          return no("record_changed");
        if (latest.some((r) => r.id !== id && adapter.sharesOwner(record, fresh.owner, r)))
          return no("shared_owner");
        return adapter.release(record, fresh);
      });
    });
  });
}
export async function notificationFacts(binary: string, includeArchived = false): Promise<Json> {
  const records = (await catalogRead()).sessions,
    agents = [
      ...array((await api(binary, "agent.list", {})).agents).map(object),
      ...(await harnessAgents(records)),
    ],
    facts: ObjectValue[] = [];
  for (const record of records) {
    if (record.archived && !includeArchived) continue;
    const agent = agents.find(
      (a) =>
        sameReference(a, record) || (!record.reference && a.terminal_id === record.terminal_id),
    );
    if (!agent || agent.launch_pending) continue;
    const reference = record.reference;
    let controls = "",
      known = true;
    if (reference?.source === "kite:harness" && ["dsh", "ply"].includes(reference.agent)) {
      try {
        const state = object(readJson(join(harnessDirectory(reference.value), "state.json")));
        if (state.turn_id && ["done", "failed", "cancelled"].includes(String(state.status)))
          controls = JSON.stringify({
            type: "event_msg",
            payload: {
              type: state.status === "done" ? "task_complete" : "task_aborted",
              turn_id: state.turn_id,
            },
          });
      } catch {
        known = false;
      }
    }
    if (reference?.agent === "codex") {
      try {
        const root = join(process.env.CODEX_HOME || join(home(), ".codex"), "sessions");
        let file: string | undefined;
        if (reference.kind === "id" && UUID.test(reference.value)) {
          const sources = historyFiles(root, reference.value);
          check(sources.length <= 1, "ambiguous source");
          file = sources[0];
        } else if (reference.kind === "path") file = reference.value;
        if (file) {
          const { realpathSync } = await import("node:fs");
          check(
            realpathSync(file).startsWith(`${realpathSync(root)}/`) && file.endsWith(".jsonl"),
            "invalid history source",
          );
          controls = (await questionRecords(file)).toString();
        }
      } catch {
        known = false;
      }
    }
    facts.push({
      session_id: record.id,
      title: [...record.title].slice(0, 160).join(""),
      source: `${agent.terminal_id}:${sha256(JSON.stringify(reference ?? {}))}`,
      status: agent.agent_status,
      revision: agent.state_change_seq ?? 0,
      thread: reference?.value ?? "",
      controls,
      controls_known: known,
      lifecycle: ["codex", "dsh", "ply"].includes(reference?.agent || ""),
    });
  }
  const result = { facts };
  check(
    Buffer.byteLength(JSON.stringify(result)) <= 4 * 1024 * 1024,
    "notification facts exceed transport capacity",
  );
  return result;
}
export const hasSavedOwner = (pane: string) => existsSync(registryPath(pane));
