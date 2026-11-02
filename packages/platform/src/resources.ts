/** Linux mechanisms only. Rust remains responsible for memory admission and thresholds. */
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { cpus, loadavg } from "node:os";
import { join } from "node:path";
import { check, integer, type Json, object, type SessionRecord, string } from "@kite/protocol";
import {
  atomic,
  exec,
  home,
  locked,
  privateDir,
  readJson,
  run,
  same,
  selfCommand,
  sleep,
  startTicks,
  tryLock,
} from "@kite/runtime";
export const SLICE = "kite-agents.slice",
  MIN_CGROUP = 16 * 1024 * 1024;
export interface ProcessFact {
  pid: number;
  ppid: number;
  started: string;
  uid: number;
  name: string;
  ticks: number;
  rss: number;
  reserved: number;
  cgroup: string;
}
export interface OwnerIdentity {
  pane: string;
  pid: number;
  start_ticks: string;
  socket?: string;
  budget_unit?: string;
  bootstrap_pid?: number;
  shell_pid?: number;
  registry_managed?: boolean;
  [key: string]: unknown;
}
interface Reservation {
  pid: number;
  started: string;
  reserved: number;
}
interface ScopeOwner {
  pane: string;
  pid: number;
  started: string;
  created_ms: number;
}
interface Ledger {
  version: 1;
  legacy: Record<string, Reservation>;
  units: Record<string, ScopeOwner>;
  limit_bytes?: number;
  high_bytes?: number;
}
export interface ResourceHooks {
  catalog(): Promise<SessionRecord[]>;
  managed(
    binary: string,
    table: Map<number, ProcessFact>,
  ): Promise<[OwnerIdentity, ProcessFact[]][]>;
  readRegistry(pane: string): OwnerIdentity;
  registryPath(pane: string): string;
  ownerLock(pane: string): string;
  sessionLock(id: string): string;
}
let hooks: ResourceHooks | undefined;
export const setResourceHooks = (value: ResourceHooks): void => {
  hooks = value;
};
function host(): ResourceHooks {
  check(hooks, "provider resource hooks unavailable");
  return hooks;
}
export const resourceRoot = () => privateDir(join(home(), ".local/state/kite/resources"));
export const scopeName = (_pane: string, pid: number, started: string) => {
  check(
    Number.isSafeInteger(pid) && pid > 1 && /^\d+$/.test(started),
    "invalid resource scope identity",
  );
  return `kite-agent-${pid}-${started}.scope`;
};
export const insideBudget = (p: ProcessFact) => p.cgroup.includes(`/${SLICE}/`);
export function proc(pid: number): ProcessFact {
  const root = `/proc/${pid}`,
    stat = readFileSync(`${root}/stat`, "utf8").split(") ").at(-1)?.trim().split(/\s+/);
  check(stat && stat.length > 19, "invalid process stat");
  const status = Object.fromEntries(
    readFileSync(`${root}/status`, "utf8")
      .trim()
      .split("\n")
      .map((line) => {
        const at = line.indexOf(":");
        return [line.slice(0, at), line.slice(at + 1).trim()];
      }),
  );
  const memory = (key: string) => Number((status[key] || "0").split(/\s+/)[0]) * 1024;
  return {
    pid,
    ppid: Number(stat[1]),
    started: string(stat[19]),
    uid: Number(status.Uid?.split(/\s+/)[0]),
    name: (status.Name || "").slice(0, 64),
    ticks: Number(stat[11]) + Number(stat[12]),
    rss: memory("VmRSS"),
    reserved: memory("VmRSS") + memory("VmSwap") + memory("VmPTE") + 262144,
    cgroup: readFileSync(`${root}/cgroup`, "utf8").trim().replace(/^0::/, ""),
  };
}
export function processes(): Map<number, ProcessFact> {
  const table = new Map<number, ProcessFact>();
  for (const file of readdirSync("/proc")) {
    if (!/^\d+$/.test(file)) continue;
    try {
      const p = proc(Number(file));
      table.set(p.pid, p);
    } catch {
      /* processes may exit while sampled */
    }
  }
  return table;
}
export function descendants(table: Map<number, ProcessFact>, roots: number[]): ProcessFact[] {
  const found = new Set(roots);
  let changed = true;
  while (changed) {
    changed = false;
    for (const [pid, p] of table)
      if (found.has(p.ppid) && !found.has(pid)) {
        found.add(pid);
        changed = true;
      }
  }
  return [...found].flatMap((pid) => {
    const p = table.get(pid);
    return p ? [p] : [];
  });
}
export function ledger(): Ledger {
  const path = join(resourceRoot(), "budget.json");
  if (!existsSync(path)) return { version: 1, legacy: {}, units: {} };
  const raw = object(readJson(path));
  check(raw.version === 1, "invalid resource ledger; original file preserved");
  const legacy = object(raw.legacy),
    units = object(raw.units);
  check(
    Object.keys(legacy).length <= 4096 && Object.keys(units).length <= 1024,
    "resource ledger capacity exceeded",
  );
  const result: Ledger = { version: 1, legacy: {}, units: {} };
  for (const [key, value] of Object.entries(legacy)) {
    const row = object(value),
      pid = integer(row.pid),
      started = string(row.started),
      reserved = integer(row.reserved);
    check(
      pid > 1 && /^\d+$/.test(started) && reserved >= 0 && key === `${pid}:${started}`,
      "invalid legacy memory reservation",
    );
    result.legacy[key] = { pid, started, reserved };
  }
  for (const [unit, value] of Object.entries(units)) {
    const row = object(value),
      pane = string(row.pane),
      pid = integer(row.pid),
      started = string(row.started);
    check(unit === scopeName(pane, pid, started), "invalid resource scope identity");
    result.units[unit] = { pane, pid, started, created_ms: integer(row.created_ms) };
  }
  if (raw.limit_bytes !== undefined) result.limit_bytes = integer(raw.limit_bytes);
  if (raw.high_bytes !== undefined) result.high_bytes = integer(raw.high_bytes);
  return result;
}
export function liveLegacy(
  value: Ledger,
  table: Map<number, ProcessFact>,
): Record<string, Reservation> {
  return Object.fromEntries(
    Object.entries(value.legacy).filter(([, r]) => table.get(r.pid)?.started === r.started),
  );
}
async function command(args: string[]): Promise<string> {
  return (await run(args, { timeout: 8000 })).stdout.toString().trim();
}
const bus = (method: string, signature: string, ...args: (string | number)[]) =>
  command([
    "busctl",
    "--user",
    "--timeout=5",
    "call",
    "org.freedesktop.systemd1",
    "/org/freedesktop/systemd1",
    "org.freedesktop.systemd1.Manager",
    method,
    signature,
    ...args.map(String),
  ]);
async function group(unit: string): Promise<string | undefined> {
  const result = await run(
    ["systemctl", "--user", "show", unit, "-p", "ControlGroup", "-p", "LoadState"],
    { allowFailure: true },
  );
  const fields = Object.fromEntries(
    result.stdout
      .toString()
      .trim()
      .split("\n")
      .filter((s) => s.includes("="))
      .map((s) => {
        const i = s.indexOf("=");
        return [s.slice(0, i), s.slice(i + 1)];
      }),
  );
  if (fields.LoadState === "not-found") return undefined;
  check(result.code === 0, "resource unit inspection failed");
  const value = fields.ControlGroup;
  if (!value) return undefined;
  const path = join("/sys/fs/cgroup", value);
  check(!value.split("/").includes("..") && existsSync(path), "invalid control group");
  return path;
}
async function attach(unit: string, items: ProcessFact[]): Promise<void> {
  const pids = items.flatMap((p) => {
    try {
      const fresh = proc(p.pid);
      return fresh.uid === process.getuid?.() && fresh.started === p.started ? [p.pid] : [];
    } catch {
      return [];
    }
  });
  if (!pids.length) return;
  const existing = await group(unit);
  if (existing) {
    check(
      existing.includes(`/${SLICE}/`) && existing.endsWith(`/${unit}`),
      "resource scope identity mismatch",
    );
    await bus("AttachProcessesToUnit", "ssau", unit, "", pids.length, ...pids);
  } else
    await bus(
      "StartTransientUnit",
      "ssa(sv)a(sa(sv))",
      unit,
      "fail",
      4,
      "Description",
      "s",
      "Kite managed agent",
      "Slice",
      "s",
      SLICE,
      "CollectMode",
      "s",
      "inactive-or-failed",
      "PIDs",
      "au",
      pids.length,
      ...pids,
      0,
    );
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline) {
    const pending = pids.filter((pid) => {
      try {
        return !proc(pid).cgroup.endsWith(`/${unit}`);
      } catch {
        return false;
      }
    });
    if (!pending.length) return;
    await sleep(20);
  }
  throw new Error("resource scope adoption was not confirmed");
}
export async function configure(
  limit: number,
  high: number,
  binary: string,
  adopt = true,
  extra?: [OwnerIdentity, ProcessFact[]],
): Promise<Json> {
  check(
    Number.isSafeInteger(limit) && limit >= MIN_CGROUP && Number.isSafeInteger(high) && high > 0,
    "invalid memory budget",
  );
  return locked(join(resourceRoot(), "budget.lock"), async () => {
    const value = ledger(),
      table = processes();
    value.legacy = liveLegacy(value, table);
    const owners = adopt ? await host().managed(binary, table) : [];
    if (extra) owners.push(extra);
    const manager = await command(["systemctl", "--user", "show", "-p", "ControlGroup", "--value"]);
    check(manager && manager !== "/", "user resource delegation unavailable");
    for (const [owner, items] of owners)
      for (const p of items) {
        check(
          p.uid === process.getuid?.() && p.cgroup.startsWith(`${manager}/`),
          "process is outside the managed user hierarchy",
        );
        if (!insideBudget(p) && p.pid !== owner.bootstrap_pid)
          value.legacy[`${p.pid}:${p.started}`] ??= {
            pid: p.pid,
            started: p.started,
            reserved: p.reserved,
          };
      }
    const reserved = Object.values(value.legacy).reduce((n, p) => n + p.reserved, 0),
      maximum = limit - reserved;
    check(maximum >= MIN_CGROUP, "agent memory budget is below existing allocations");
    const existing = await group(SLICE),
      current = existing ? readInteger(join(existing, "memory.current")) : 0;
    check(maximum >= current + MIN_CGROUP, "agent memory budget cannot shrink below current use");
    value.limit_bytes = limit;
    value.high_bytes = high;
    atomic(join(resourceRoot(), "budget.json"), value);
    await command([
      "systemctl",
      "--user",
      "set-property",
      "--runtime",
      SLICE,
      "MemoryAccounting=yes",
      `MemoryMax=${maximum}`,
      `MemoryHigh=${Math.max(MIN_CGROUP, Math.min(maximum, high - reserved))}`,
      "MemorySwapMax=0",
    ]);
    await command(["systemctl", "--user", "start", SLICE]);
    const path = await group(SLICE);
    check(
      path && readFileSync(join(path, "memory.max"), "utf8").trim() === String(maximum),
      "kernel memory limit was not applied",
    );
    for (const [owner, items] of owners) {
      const unit = scopeName(owner.pane, owner.pid, owner.start_ticks),
        oldGroups = new Set(
          items
            .filter((p) => p.pid === owner.pid || p.pid === owner.shell_pid)
            .map((p) => p.cgroup),
        ),
        stable = [...oldGroups].every((g) => g.endsWith(`/${unit}`));
      await attach(
        unit,
        owner.bootstrap_pid
          ? items
          : items.filter((p) => !insideBudget(p) || (!stable && oldGroups.has(p.cgroup))),
      );
      value.units[unit] = {
        pane: owner.pane,
        pid: owner.pid,
        started: owner.start_ticks,
        created_ms: Date.now(),
      };
      if (!owner.bootstrap_pid && owner.registry_managed !== false) {
        await locked(host().ownerLock(owner.pane), () => {
          const fresh = host().readRegistry(owner.pane);
          check(
            fresh.pid === owner.pid && fresh.start_ticks === owner.start_ticks,
            "agent owner changed during resource adoption",
          );
          if (fresh.budget_unit !== unit)
            atomic(host().registryPath(owner.pane), { ...fresh, budget_unit: unit });
        });
      }
    }
    atomic(join(resourceRoot(), "budget.json"), value);
    return { applied: true, reserved_bytes: reserved, effective_limit_bytes: maximum };
  });
}
export async function killScope(unit: string): Promise<void> {
  const path = await group(unit);
  if (!path) return;
  check(
    /^kite-agent-\d+-\d+\.scope$/.test(unit) && path.includes(`/${SLICE}/`),
    "unmanaged resource scope",
  );
  await bus("KillUnit", "ssi", unit, "all", 15);
  const populated = () => {
    try {
      return Boolean(readFileSync(join(path, "cgroup.procs"), "utf8").trim());
    } catch {
      return false;
    }
  };
  const deadline = Date.now() + 3000;
  while (populated() && Date.now() < deadline) await sleep(50);
  if (populated()) await bus("KillUnit", "ssi", unit, "all", 9);
}
export async function stopOwner(owner: OwnerIdentity): Promise<void> {
  const current = host().readRegistry(owner.pane),
    unit = scopeName(owner.pane, owner.pid, owner.start_ticks);
  check(
    same(current, owner) &&
      owner.budget_unit === unit &&
      proc(owner.pid).cgroup.endsWith(`/${unit}`),
    "resource owner identity changed",
  );
  await killScope(unit);
}
export async function reapOrphans(): Promise<void> {
  await locked(join(resourceRoot(), "budget.lock"), async () => {
    const value = ledger(),
      table = processes(),
      records = await host().catalog();
    for (const [unit, owner] of Object.entries(value.units)) {
      if (table.get(owner.pid)?.started === owner.started) continue;
      const leases: (() => void)[] = [];
      try {
        let blocked = false;
        for (const record of records)
          if (record.pane_id === owner.pane) {
            const lease = tryLock(host().sessionLock(record.id));
            if (!lease) {
              blocked = true;
              break;
            }
            leases.push(lease);
          }
        if (blocked) continue;
        let replacement: OwnerIdentity | undefined;
        try {
          replacement = host().readRegistry(owner.pane);
        } catch {
          /* no replacement */
        }
        if (
          replacement &&
          replacement.pid !== owner.pid &&
          (!replacement.budget_unit || replacement.budget_unit === unit)
        )
          continue;
        await killScope(unit);
        delete value.units[unit];
      } finally {
        for (const release of leases) release();
      }
    }
    value.legacy = liveLegacy(value, table);
    atomic(join(resourceRoot(), "budget.json"), value);
  });
}
function readInteger(path: string): number {
  try {
    const n = Number(readFileSync(path, "utf8").trim());
    return Number.isFinite(n) ? n : 0;
  } catch {
    return 0;
  }
}
function pairs(path: string): Record<string, number> {
  try {
    return Object.fromEntries(
      readFileSync(path, "utf8")
        .trim()
        .split("\n")
        .map((s) => {
          const [k, v] = s.split(/\s+/);
          return [k, Number(v)];
        }),
    );
  } catch {
    return {};
  }
}
export async function sample(): Promise<Json> {
  const mem = Object.fromEntries(
    readFileSync("/proc/meminfo", "utf8")
      .trim()
      .split("\n")
      .map((s) => [s.split(":")[0], Number(s.trim().split(/\s+/)[1]) * 1024]),
  );
  const cpu = (readFileSync("/proc/stat", "utf8").split("\n")[0] || "")
      .trim()
      .split(/\s+/)
      .slice(1, 9)
      .map(Number),
    pressure: Record<string, Record<string, number>> = {};
  for (const line of readFileSync("/proc/pressure/memory", "utf8").trim().split("\n")) {
    const [name, ...values] = line.split(/\s+/);
    if (name)
      pressure[name] = Object.fromEntries(
        values.map((v) => {
          const [k, n] = v.split("=");
          return [k, Number(n)];
        }),
      );
  }
  const table = processes(),
    value = ledger(),
    reserved = Object.values(liveLegacy(value, table)).reduce((n, p) => n + p.reserved, 0);
  const uncontained = new Set<number>();
  for (const owner of Object.values(value.units)) {
    const p = table.get(owner.pid);
    if (p && p.started === owner.started && !insideBudget(p)) uncontained.add(p.pid);
  }
  if (hooks)
    for (const record of await hooks.catalog()) {
      if (record.provider !== "codex" || !record.pane_id) continue;
      try {
        const owner = hooks.readRegistry(record.pane_id),
          p = table.get(owner.pid);
        if (p && p.started === owner.start_ticks && !insideBudget(p)) uncontained.add(p.pid);
      } catch {
        /* stale registries are not live owners */
      }
    }
  const manager =
      table.get(process.pid)?.cgroup.split("/app.slice/")[0] ||
      (await command(["systemctl", "--user", "show", "-p", "ControlGroup", "--value"])),
    budget = join("/sys/fs/cgroup", manager, "kite.slice", SLICE),
    events = pairs(join(budget, "memory.events")),
    stats = pairs(join(budget, "cpu.stat")),
    memory = pairs(join(budget, "memory.stat")),
    current = readInteger(join(budget, "memory.current"));
  return {
    sampled_at_ms: Date.now(),
    cpu_total_ticks: cpu.reduce((n, v) => n + v, 0),
    cpu_idle_ticks: (cpu[3] || 0) + (cpu[4] || 0),
    cpu_cores: cpus().length || 1,
    load_average: loadavg(),
    memory_total_bytes: mem.MemTotal ?? 0,
    memory_available_bytes: mem.MemAvailable ?? 0,
    swap_total_bytes: mem.SwapTotal ?? 0,
    swap_used_bytes: (mem.SwapTotal || 0) - (mem.SwapFree || 0),
    memory_pressure_some: pressure.some?.avg10 ?? 0,
    memory_pressure_full: pressure.full?.avg10 ?? 0,
    agent_current_bytes: current,
    agent_reclaimable_bytes: Math.min(current, memory.inactive_file || 0),
    agent_reserved_bytes: reserved,
    agent_effective_limit_bytes: readInteger(join(budget, "memory.max")),
    agent_cpu_usage_usec: stats.usage_usec ?? 0,
    agent_process_count: [...table.values()].filter(insideBudget).length,
    agent_oom_kills: events.oom_kill ?? 0,
    agent_high_events: events.high ?? 0,
    uncontained_owners: uncontained.size,
    enforced: existsSync(budget) && readInteger(join(budget, "memory.max")) > 0,
    processes: [...table.values()]
      .sort((a, b) => b.rss - a.rss)
      .slice(0, 8)
      .map((p) => ({
        pid: p.pid,
        name: p.name,
        started: p.started,
        cpu_ticks: p.ticks,
        memory_bytes: p.rss,
        managed: insideBudget(p),
      })),
  };
}
export async function node(...args: string[]): Promise<Json> {
  return JSON.parse(
    (await run([join(home(), ".local/bin/kite-node"), ...args])).stdout.toString(),
  ) as Json;
}
export async function ready(owner: OwnerIdentity): Promise<void> {
  if (owner.budget_unit)
    try {
      await node("resource-ready", "--pid", String(owner.pid));
    } catch {
      /* startup reservation expires during handover */
    }
}
export async function reserveExisting(owner: OwnerIdentity): Promise<void> {
  if (process.env.KITE_AGENT_RESOURCE_CONTROL !== "1") return;
  const admission = object(
    await node("resource-admit", "--pane", owner.pane, "--pid", String(owner.pid)),
  );
  check(admission.allowed, "agent resource admission denied");
}
export async function admit(pane: string, pid: number, shellPid?: number): Promise<OwnerIdentity> {
  const admission = object(await node("resource-admit", "--pane", pane, "--pid", String(pid)));
  check(admission.allowed, "agent resource admission denied");
  const ticks = startTicks(pid);
  check(ticks, "resource owner unavailable");
  const owner: OwnerIdentity = {
    pane,
    pid,
    start_ticks: ticks,
    registry_managed: false,
    ...(pid === process.pid ? { bootstrap_pid: pid } : {}),
    ...(shellPid ? { shell_pid: shellPid } : {}),
  };
  await configure(integer(admission.limit_bytes), integer(admission.high_bytes), "", false, [
    owner,
    descendants(processes(), [pid, ...(shellPid ? [shellPid] : [])]),
  ]);
  return { ...owner, budget_unit: scopeName(pane, pid, ticks) };
}
export async function execBudgeted(
  pane: string,
  command: string[],
  shellPid?: number,
): Promise<never> {
  await admit(pane, process.pid, shellPid);
  return exec(command);
}
export const budgetCommand = (pane: string, command: string[]) =>
  selfCommand("resources", "exec-harness", "--pane", pane, "--", ...command);
