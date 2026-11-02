import { existsSync, readFileSync, statSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import { stopOwner } from "@kite/platform";
import {
  array,
  check,
  type Json,
  json,
  type ObjectValue,
  object,
  type SessionRecord,
  string,
  UUID,
} from "@kite/protocol";
import {
  clockTicks,
  home,
  live,
  locked,
  pgid,
  readJson,
  same,
  sha256,
  sleep,
  startTicks,
  stopped,
} from "@kite/runtime";
import { catalogRead, sessionLock } from "./catalog";
import {
  api,
  backendRpc,
  ownerLock,
  type Registry,
  readRegistry,
  registryPath,
  sameReference,
} from "./codex";
import { harnessAgents, harnessDirectory } from "./harness";
import { historyFiles, questionRecords } from "./history";

interface LifecycleFact {
  session_id: string;
  pane: string;
  owner: Registry;
  reference: SessionRecord["reference"];
  archived: boolean;
  idle: boolean;
  persisted: boolean;
  activity: string;
  last_activity_ms: number;
  tui_pid: number | null;
}
export async function lifecycleFact(
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
  const boot = Number(
      readFileSync("/proc/stat", "utf8")
        .split("\n")
        .find((s) => s.startsWith("btime "))
        ?.split(/\s+/)[1],
    ),
    startedMs = Math.trunc((boot + Number(owner.start_ticks) / clockTicks()) * 1000),
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
export async function lifecycleFacts(binary: string): Promise<Json> {
  const records = (await catalogRead()).sessions,
    agents = array((await api(binary, "agent.list", {})).agents).map(object),
    facts: LifecycleFact[] = [];
  for (const r of records) {
    if (r.provider !== "codex" || !r.pane_id) continue;
    try {
      facts.push(await lifecycleFact(r, binary, agents));
    } catch {
      /* unavailable facts never authorize stopping */
    }
  }
  return json({ facts });
}
export async function lifecycleReap(expected: ObjectValue, binary: string): Promise<Json> {
  const id = string(expected.session_id),
    pane = string(expected.pane),
    reason = string(expected.reason);
  check(["archived", "idle_timeout"].includes(reason), "invalid lifecycle request");
  return locked(sessionLock(id), () =>
    locked(ownerLock(pane), async () => {
      const catalog = await catalogRead(),
        record = catalog.sessions.find((r) => r.id === id);
      const no = (why: string) => ({ reclaimed: false, reason: why });
      if (!record || record.pane_id !== pane || record.provider !== "codex")
        return no("record_changed");
      if (reason === "archived" && !record.archived) return no("unarchived");
      const fresh = await lifecycleFact(record, binary);
      for (const field of ["owner", "reference", "activity", "archived"] as const)
        if (!same(fresh[field], expected[field])) return no("activity_changed");
      if (!fresh.idle || !fresh.persisted) return no("not_idle_or_not_persisted");
      const owner = fresh.owner;
      for (const other of catalog.sessions) {
        if (other.id === id || !other.pane_id) continue;
        try {
          const alias = readRegistry(other.pane_id);
          if (alias.pid === owner.pid && alias.start_ticks === owner.start_ticks)
            return no("shared_owner");
        } catch {
          /* no alias */
        }
      }
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
      const after = await lifecycleFact(record, binary);
      if (
        ["owner", "reference", "activity"].some(
          (field) => !same(object(json(after))[field], object(json(fresh))[field]),
        ) ||
        !after.idle
      )
        return no("activity_changed");
      if (!same(readRegistry(pane), owner) || pgid(owner.pid) !== owner.pid)
        return no("owner_changed");
      if (owner.budget_unit) await stopOwner(owner);
      else process.kill(-owner.pid, "SIGTERM");
      const deadline = Date.now() + 4000;
      while (live(owner.pid, owner.start_ticks) && Date.now() < deadline) await sleep(50);
      if (live(owner.pid, owner.start_ticks)) return no("owner_still_running");
      if (same(readJson(registryPath(pane)), owner)) unlinkSync(registryPath(pane));
      return { reclaimed: true, session_id: id };
    }),
  );
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
