/** Explicit deletion stops the selected owner, including active work, before removing its record. */
import { existsSync, readFileSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import {
  killScope,
  type OwnerIdentity,
  processes,
  reclaimCache,
  scopeName,
  stopOwner,
} from "@kite/platform";
import {
  array,
  type Catalog,
  check,
  type ObjectValue,
  object,
  type Provider,
  type Reference,
  type SessionRecord,
  string,
} from "@kite/protocol";
import { live, locked, pgid, readJson, same, sleep, startTicks } from "@kite/runtime";
import { listening } from "@kite/runtime/socket";
import {
  catalogRead,
  catalogRoot,
  catalogUnlocked,
  markSessionDeleted,
  saveCatalogUnlocked,
  sessionLock,
} from "./catalog";
import { api, backendRpc, ownerLock, readRegistry, registryPath, sameReference } from "./codex";
import { harnessDirectory, harnessInfo } from "./harness";

export interface SessionTerminator {
  ownerLock(record: SessionRecord): string;
  sharesOwner(record: SessionRecord, other: SessionRecord): boolean;
  terminate(record: SessionRecord, peers: SessionRecord[]): Promise<Reference | undefined>;
}
function savedOwner(pane: string): OwnerIdentity | undefined {
  if (!existsSync(registryPath(pane))) return;
  const value = object(readJson(registryPath(pane)));
  check(value.pane === pane, "backend pane identity changed");
  return { ...value, pane, pid: Number(value.pid), start_ticks: string(value.start_ticks) };
}
async function waitStopped(owner: OwnerIdentity): Promise<void> {
  const deadline = Date.now() + 4000;
  while (live(owner.pid, owner.start_ticks) && Date.now() < deadline) await sleep(50);
  check(!live(owner.pid, owner.start_ticks), "session owner is still running");
}
async function stopProcessGroup(owner: OwnerIdentity): Promise<void> {
  check(
    live(owner.pid, owner.start_ticks) && pgid(owner.pid) === owner.pid,
    "session process group changed",
  );
  const members = () =>
    [...processes().values()].filter(
      (p) => p.uid === process.getuid?.() && pgid(p.pid) === owner.pid,
    );
  const remaining = members();
  process.kill(-owner.pid, "SIGTERM");
  const deadline = Date.now() + 1500;
  while (remaining.some((p) => live(p.pid, p.started)) && Date.now() < deadline) await sleep(50);
  // A group leader can exit before its tools; track their start identities and stop those too.
  if (remaining.some((p) => live(p.pid, p.started))) {
    for (const p of members())
      if (!remaining.some((old) => old.pid === p.pid && old.started === p.started))
        remaining.push(p);
  }
  for (const p of remaining) if (live(p.pid, p.started)) process.kill(p.pid, "SIGKILL");
  for (const p of remaining)
    await waitStopped({ pane: owner.pane, pid: p.pid, start_ticks: p.started });
}
async function terminateTerminal(
  record: SessionRecord,
  binary: string,
  owner?: OwnerIdentity,
): Promise<void> {
  const pane = record.pane_id;
  if (!pane) return;
  let info: ObjectValue;
  try {
    info = object((await api(binary, "pane.process_info", { pane_id: pane })).process_info);
  } catch (error) {
    if (error instanceof Error && error.message.includes("not_found")) return;
    throw error;
  }
  if (info.foreground_process_group_id === info.shell_pid) return;
  const agents = array((await api(binary, "agent.list", {})).agents).map(object);
  check(
    agents.some(
      (a) =>
        a.pane_id === pane &&
        (sameReference(a, record) ||
          (!record.reference &&
            a.terminal_id === record.terminal_id &&
            (a.agent === record.provider || a.launch_pending))),
    ),
    "terminal session identity changed",
  );
  const front = array(info.foreground_processes).map(object);
  const pid = Number(info.foreground_process_group_id),
    ticks = startTicks(pid);
  check(
    pid > 1 &&
      pid !== info.shell_pid &&
      ticks &&
      pgid(pid) === pid &&
      front.some((p) => p.pid === pid),
    "terminal process identity changed",
  );
  const args = readFileSync(`/proc/${pid}/cmdline`).toString().split("\0");
  if (record.provider === "codex")
    check(
      front.some((p) => p.pid === pid && p.name === "codex") &&
        args.includes("--remote") &&
        owner?.socket &&
        args.includes(`unix://${owner.socket}`),
      "terminal backend is not owned by this session",
    );
  // A managed terminal shares the owner's scope, which also includes local tool descendants.
  if (
    owner?.budget_unit &&
    readFileSync(`/proc/${pid}/cgroup`, "utf8").trim().endsWith(`/${owner.budget_unit}`)
  )
    return;
  check(record.provider === "codex" && owner, "terminal has no managed resource owner");
  await stopProcessGroup({ pane, pid, start_ticks: ticks });
}
export function sessionTerminators(binary: string): Record<Provider, SessionTerminator> {
  const terminal: SessionTerminator = {
    ownerLock: (record) => ownerLock(string(record.pane_id)),
    sharesOwner: (record, other) => {
      if (!record.pane_id || !other.pane_id || other.provider !== "codex") return false;
      const a = savedOwner(record.pane_id),
        b = savedOwner(other.pane_id);
      return Boolean(a && b && a.pid === b.pid && a.start_ticks === b.start_ticks);
    },
    terminate: async (record, peers) => {
      const pane = string(record.pane_id),
        saved = record.provider === "codex" ? savedOwner(pane) : undefined;
      let owner: OwnerIdentity | undefined;
      let reference: Reference | undefined;
      if (saved && live(saved.pid, saved.start_ticks)) {
        owner = readRegistry(pane);
        const loaded = array(object(await backendRpc(pane, "thread/loaded/list", {}, 2000)).data);
        check(
          loaded.length <= 1 &&
            (!loaded.length || !record.reference || loaded[0] === record.reference.value),
          "session backend is shared or its thread changed",
        );
        if (!record.reference && loaded.length === 1)
          reference = {
            source: "herdr:codex",
            agent: "codex",
            kind: "id",
            value: string(loaded[0]),
          };
        check(
          !reference || !peers.some((r) => same(r.reference, reference)),
          "session thread is shared",
        );
      } else if (saved?.socket)
        check(!(await listening(saved.socket)), "session backend identity changed");
      if (record.provider === "claude") {
        // Claude's managed scope is rooted in its verified pane shell, not an app-server.
        let info: ObjectValue;
        try {
          info = object((await api(binary, "pane.process_info", { pane_id: pane })).process_info);
        } catch (error) {
          if (error instanceof Error && error.message.includes("not_found")) return;
          throw error;
        }
        const pid = Number(info.shell_pid),
          ticks = startTicks(pid);
        check(ticks, "terminal shell identity unavailable");
        const unit = scopeName(pane, pid, ticks);
        if (
          readFileSync(`/proc/${pid}/cgroup`, "utf8").trim().endsWith(`/kite-agents.slice/${unit}`)
        ) {
          owner = { pane, pid, start_ticks: ticks, budget_unit: unit };
        }
      }
      await terminateTerminal(record, binary, owner);
      if (!owner) return;
      if (record.provider === "codex") {
        check(pgid(owner.pid) === owner.pid, "backend process group changed");
        if (owner.budget_unit) await stopOwner(owner);
        else await stopProcessGroup(owner);
      } else {
        await reclaimCache(string(owner.budget_unit));
        await killScope(string(owner.budget_unit));
      }
      await waitStopped(owner);
      if (record.provider === "codex" && same(readJson(registryPath(pane)), owner))
        unlinkSync(registryPath(pane));
      return reference;
    },
  };
  const managed: SessionTerminator = {
    ownerLock: (record) => join(harnessDirectory(string(record.reference?.value)), "owner.lock"),
    sharesOwner: (record, other) => same(record.reference, other.reference),
    terminate: async (record): Promise<Reference | undefined> => {
      const ref = record.reference;
      check(
        ref?.source === "kite:harness" && ref.agent === record.provider && ref.kind === "id",
        "unmanaged harness reference",
      );
      const path = harnessDirectory(ref.value),
        info = harnessInfo(readJson(join(path, "session.json")));
      check(
        info.id === record.id && info.thread === ref.value && info.provider === record.provider,
        "harness session identity changed",
      );
      const stateFile = join(path, "state.json");
      if (!existsSync(stateFile)) {
        check(!(await listening(join(path, "owner.sock"))), "harness identity unavailable");
        return;
      }
      const state = object(readJson(stateFile)),
        pid = Number(state.pid),
        ticks = string(state.start_ticks);
      if (!live(pid, ticks)) {
        check(!(await listening(join(path, "owner.sock"))), "harness owner identity changed");
        return;
      }
      const args = readFileSync(`/proc/${pid}/cmdline`).toString().split("\0"),
        unit = scopeName(string(record.pane_id), pid, ticks);
      check(
        args.includes("harness-owner") &&
          args.includes(ref.value) &&
          readFileSync(`/proc/${pid}/cgroup`, "utf8").trim().endsWith(`/kite-agents.slice/${unit}`),
        "harness is outside its managed scope",
      );
      await reclaimCache(unit);
      await killScope(unit);
      await waitStopped({ pane: string(record.pane_id), pid, start_ticks: ticks });
      return undefined;
    },
  };
  return { codex: terminal, claude: terminal, dsh: managed, ply: managed };
}
export async function sessionDelete(
  id: string,
  binary: string,
  adapters: Partial<Record<Provider, SessionTerminator>> = sessionTerminators(binary),
): Promise<Catalog> {
  return locked(sessionLock(id), async () => {
    const record = (await catalogRead()).sessions.find((r) => r.id === id);
    if (!record) return catalogRead();
    const adapter = adapters[record.provider];
    const remove = () =>
      locked(join(catalogRoot(), "sessions.lock"), async () => {
        const catalog = catalogUnlocked(),
          current = catalog.sessions.find((r) => r.id === id);
        check(same(record, current), "session changed during deletion");
        check(
          !catalog.sessions.some(
            (r) =>
              r.id !== id &&
              ((record.reference && same(record.reference, r.reference)) ||
                (record.pane_id && r.pane_id === record.pane_id) ||
                adapter?.sharesOwner(record, r)),
          ),
          "session owner is shared",
        );
        if (record.pane_id) {
          check(adapter, "session termination is unavailable");
          const reference = await adapter.terminate(
            record,
            catalog.sessions.filter((r) => r.id !== id),
          );
          if (reference) markSessionDeleted({ ...record, reference });
        }
        markSessionDeleted(record);
        return saveCatalogUnlocked({
          ...catalog,
          sessions: catalog.sessions.filter((r) => r.id !== id),
        });
      });
    return record.pane_id && adapter ? locked(adapter.ownerLock(record), remove) : remove();
  });
}
