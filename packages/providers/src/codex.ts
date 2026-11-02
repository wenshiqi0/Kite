import { chmodSync, closeSync, constants, existsSync, lstatSync, openSync } from "node:fs";
import { basename, isAbsolute, join } from "node:path";
import {
  admit,
  descendants,
  type OwnerIdentity,
  type ProcessFact,
  ready,
  reserveExisting,
  scopeName,
  setResourceHooks,
} from "@kite/platform";
import {
  array,
  check,
  integer,
  type Json,
  type ObjectValue,
  object,
  optionalString,
  reference as parseReference,
  type SessionRecord,
  string,
  UUID,
} from "@kite/protocol";
import {
  atomic,
  entry,
  home,
  locked,
  privateDir,
  processArgv,
  processContext,
  readJson,
  run,
  same,
  selfCommand,
  sha256,
  sleep,
  startTicks,
} from "@kite/runtime";
import { RpcProcess } from "@kite/runtime/rpc";
import { bridgeSocket, listening, requirePrivateSocket } from "@kite/runtime/socket";
import { catalogRead, patchSession, sessionLock, stateDir } from "./catalog";
import { harnessDirectory, harnessInfo, openHarness } from "./harness";
import { historyFiles } from "./history";
import { websocketProxy } from "./websocket";
export interface Registry extends OwnerIdentity {
  protocol: 1;
  socket: string;
  last_open_ms?: number;
}
export function paneKey(pane: string): string {
  check(/^[A-Za-z0-9_-]+:p\d+$/.test(pane), "invalid pane ID");
  return sha256(pane).slice(0, 20);
}
export const registryPath = (pane: string) => join(stateDir(), `${paneKey(pane)}.json`);
export const ownerLock = (pane: string) => join(stateDir(), `${paneKey(pane)}.lock`);
export function readRegistry(pane: string): Registry {
  const row = object(readJson(registryPath(pane))),
    pid = Number(row.pid),
    ticks = string(row.start_ticks),
    socket = string(row.socket);
  check(row.protocol === 1 && row.pane === pane, "backend protocol mismatch");
  check(startTicks(pid) === ticks, "backend process changed");
  const args = processArgv(pid);
  check(
    args.includes("app-server") && args.includes(`unix://${socket}`),
    "backend identity mismatch",
  );
  requirePrivateSocket(socket);
  return {
    ...row,
    protocol: 1,
    pane,
    pid,
    start_ticks: ticks,
    socket,
    ...(row.last_open_ms === undefined ? {} : { last_open_ms: integer(row.last_open_ms) }),
    ...(row.budget_unit === undefined ? {} : { budget_unit: string(row.budget_unit) }),
  };
}
export async function api(binary: string, method: string, params: Json): Promise<ObjectValue> {
  const result = object(
    JSON.parse(
      (
        await run([binary, "remote-api-bridge"], {
          input: `${JSON.stringify({ id: "kite-backend", method, params })}\n`,
          timeout: 15000,
        })
      ).stdout.toString(),
    ) as unknown,
  );
  if (result.error) throw new Error(`herdr ${JSON.stringify(result.error)}`);
  return object(result.result);
}
export async function ensure(pane: string, binary: string): Promise<Registry> {
  try {
    return readRegistry(pane);
  } catch {
    /* inspect under lease */
  }
  return locked(ownerLock(pane), async () => {
    try {
      return readRegistry(pane);
    } catch {
      /* stale registry */
    }
    const version = (await run(["codex", "--version"], { timeout: 10000 })).stdout.toString(),
      match = /(\d+)\.(\d+)\.(\d+)/.exec(version);
    const minimum = process.platform === "darwin" ? 166 : 164;
    check(
      match && (Number(match[1]) > 0 || Number(match[2]) >= minimum),
      `Codex 0.${minimum}.0 or newer is required`,
    );
    const processInfo = object(
        (await api(binary, "pane.process_info", { pane_id: pane })).process_info,
      ),
      shell = Number(processInfo.shell_pid),
      context = processContext(shell),
      cwd = context.cwd;
    const env: NodeJS.ProcessEnv = { ...process.env, KITE_HOST_BACKEND_PATH: entry() };
    delete env.CODEX_THREAD_ID;
    if (process.platform === "darwin") {
      // SIP-protected shells can omit their environment from KERN_PROCARGS2.
      // Derive hook context from the verified pane API and configured server.
      const server = object(
        JSON.parse(
          (await run([binary, "status", "server", "--json"], { timeout: 5000 })).stdout.toString(),
        ) as unknown,
      );
      const socket = string(server.socket);
      check(
        server.running === true &&
          server.compatible === true &&
          isAbsolute(socket) &&
          lstatSync(socket).isSocket() &&
          lstatSync(socket).uid === process.getuid?.(),
        "Herdr server identity unavailable",
      );
      env.HERDR_PANE_ID = pane;
      env.HERDR_SOCKET_PATH = socket;
    } else {
      for (const [name, value] of Object.entries(context.environment)) {
        if (name.startsWith("HERDR_")) env[name] = value;
      }
    }
    check(env.HERDR_PANE_ID === pane && env.HERDR_SOCKET_PATH, "pane environment unavailable");
    const runtime = `/run/user/${process.getuid?.()}`,
      root = privateDir(join(existsSync(runtime) ? runtime : stateDir(), "kite")),
      socket = join(root, `codex-${paneKey(pane)}.sock`);
    check(!(await listening(socket)), "unregistered live backend; inspect before retrying");
    const fd = openSync(
      join(stateDir(), `${paneKey(pane)}.log`),
      constants.O_WRONLY | constants.O_CREAT | constants.O_APPEND,
      0o600,
    );
    const launch = [
      "codex",
      "-c",
      "check_for_update_on_startup=false",
      "app-server",
      "--listen",
      `unix://${socket}`,
    ];
    const command =
      process.platform === "darwin"
        ? launch
        : selfCommand(
            "resources",
            "exec-agent",
            "--pane",
            pane,
            "--herdr",
            binary,
            "--",
            "codex",
            "-c",
            "check_for_update_on_startup=false",
            "app-server",
            "--listen",
            `unix://${socket}`,
          );
    let child: Bun.Subprocess;
    try {
      child = Bun.spawn(command, {
        stdin: "ignore",
        stdout: fd,
        stderr: fd,
        env,
        cwd,
        detached: true,
      });
      child.unref();
    } finally {
      closeSync(fd);
    }
    const ticks = startTicks(child.pid);
    check(ticks, "backend process exited");
    const info: Registry = {
      protocol: 1,
      pane,
      socket,
      pid: child.pid,
      start_ticks: ticks,
      last_open_ms: Date.now(),
      ...(process.platform === "linux" ? { budget_unit: scopeName(pane, child.pid, ticks) } : {}),
    };
    for (let i = 0; i < 100; i++) {
      check(child.exitCode === null, "Codex backend exited; inspect private host log");
      if (await listening(socket)) {
        chmodSync(socket, 0o600);
        atomic(registryPath(pane), info);
        return readRegistry(pane);
      }
      await sleep(100);
    }
    child.kill();
    throw new Error("Codex backend startup timed out");
  });
}
export async function backendRpc(
  pane: string,
  method: string,
  params: Json,
  timeout = 10000,
): Promise<Json> {
  const rpc = new RpcProcess(selfCommand("proxy", "--pane", pane), home());
  try {
    await rpc.call(
      "initialize",
      { clientInfo: { name: "kite-host", version: "1" }, capabilities: { experimentalApi: true } },
      timeout,
    );
    rpc.write({ method: "initialized" });
    return await rpc.call(method, params, timeout);
  } finally {
    rpc.close();
  }
}
async function createMacThread(pane: string, cwd: string, title: string): Promise<string> {
  const rpc = new RpcProcess(selfCommand("proxy", "--pane", pane), home());
  try {
    await rpc.call("initialize", {
      clientInfo: { name: "kite-host", version: "1" },
      capabilities: { experimentalApi: true },
    });
    rpc.write({ method: "initialized" });
    const thread = string(
      object(
        object(await rpc.call("thread/start", { cwd, historyMode: "legacy", serviceName: "kite" }))
          .thread,
      ).id,
    );
    check(UUID.test(thread), "invalid new Codex thread identity");
    // Naming persists an otherwise empty legacy rollout. Keep the creating
    // connection alive until this succeeds so the TUI can resume it immediately.
    await rpc.call("thread/name/set", { threadId: thread, name: title || "Kite" });
    const saved = object(
      object(await rpc.call("thread/read", { threadId: thread, includeTurns: false })).thread,
    );
    check(
      saved.id === thread && typeof saved.path === "string",
      "new Codex thread was not persisted",
    );
    return thread;
  } finally {
    rpc.close();
  }
}
export async function reportSessionBinding(
  pane: string,
  binary: string,
  thread: string,
): Promise<void> {
  await api(binary, "pane.report_agent_session", {
    pane_id: pane,
    source: "herdr:codex",
    agent: "codex",
    agent_session_id: thread,
    // Herdr's native SessionStart hook orders reports by Unix nanoseconds.
    seq: Date.now() * 1_000_000,
    session_start_source: "resume",
    // Herdr persists only a bare command name, never an interpreter path.
    resume_argv: [
      "kite-node",
      "backend",
      "tui",
      "--pane",
      pane,
      "--herdr",
      binary,
      "--thread",
      thread,
    ],
  });
  const agents = array((await api(binary, "agent.list", {})).agents).map(object);
  check(
    agents.some(
      (agent) =>
        agent.pane_id === pane &&
        same(agent.agent_session, {
          source: "herdr:codex",
          agent: "codex",
          kind: "id",
          value: thread,
        }),
    ),
    "Herdr did not apply the verified Codex session binding",
  );
}
export async function bind(pane: string, binary: string, expected?: string): Promise<Json> {
  const threads = array(object(await backendRpc(pane, "thread/loaded/list", {})).data).map(string);
  let thread: string;
  if (expected) {
    check(threads.includes(expected), "expected pane-owned Codex thread is not loaded");
    thread = expected;
  } else {
    check(threads.length === 1, "expected exactly one pane-owned Codex thread");
    thread = string(threads[0]);
  }
  await reportSessionBinding(pane, binary, thread);
  await ready(readRegistry(pane));
  return { thread };
}
export function sameReference(agent: ObjectValue, record: SessionRecord): boolean {
  return Boolean(
    record.reference &&
      record.reference.agent === record.provider &&
      same(agent.agent_session, record.reference),
  );
}
export async function noteSessionOpen(pane: string): Promise<void> {
  if (!existsSync(registryPath(pane))) return;
  await locked(ownerLock(pane), () => {
    const owner = readRegistry(pane);
    atomic(registryPath(pane), { ...owner, last_open_ms: Date.now() });
  });
}
export async function proxy(pane: string): Promise<void> {
  if (pane.startsWith("kite-harness-")) {
    const path = harnessDirectory(pane.slice("kite-harness-".length));
    await bridgeSocket(join(path, "owner.sock"));
    return;
  }
  await websocketProxy(readRegistry(pane).socket);
}
export async function sessionOpen(id: string, binary: string): Promise<Json> {
  return locked(sessionLock(id), async () => {
    const found = (await catalogRead()).sessions.find((r) => r.id === id);
    check(found, "session not found");
    let record: SessionRecord = found;
    const provider = record.provider;
    if (provider === "dsh" || provider === "ply") {
      const managed = await openHarness(record);
      await patchSession(id, {
        reference: parseReference(managed.agent_session),
        terminal_id: string(managed.terminal_id),
        pane_id: string(managed.pane_id),
        workspace_id: string(managed.workspace_id),
        agent_name: optionalString(managed.name),
      });
      return { agent: managed };
    }
    check(Bun.which(provider), "backend is not installed or connected");
    let agents = array((await api(binary, "agent.list", {})).agents).map(object);
    const existing = agents.find((a) => sameReference(a, record));
    if (existing) {
      if (provider === "codex") await noteSessionOpen(string(existing.pane_id));
      return { agent: existing };
    }
    const pending = agents.find(
      (a) =>
        record.terminal_id &&
        a.terminal_id === record.terminal_id &&
        (a.launch_pending || a.agent === provider),
    );
    if (pending) {
      if (provider === "codex")
        try {
          await bind(string(pending.pane_id), binary, record.reference?.value);
        } catch {
          /* inspect lost start without replay */
        }
      agents = array((await api(binary, "agent.list", {})).agents).map(object);
      const current = agents.find((a) => a.terminal_id === pending.terminal_id) ?? pending;
      check(
        !record.reference || sameReference(current, record),
        "session identity changed; retry when the original owner is ready",
      );
      return { agent: current };
    }
    const oldPane = record.pane_id;
    let owner: Registry | undefined;
    if (provider === "codex" && oldPane && existsSync(registryPath(oldPane))) {
      const saved = object(readJson(registryPath(oldPane)));
      let candidate: Registry | undefined;
      try {
        candidate = readRegistry(oldPane);
      } catch {
        check(
          startTicks(Number(saved.pid)) !== saved.start_ticks &&
            !(await listening(string(saved.socket))),
          "existing session backend is unavailable; inspect it before restoring",
        );
      }
      if (candidate) {
        const loaded = array(object(await backendRpc(oldPane, "thread/loaded/list", {})).data);
        if (!loaded.length || (record.reference && loaded.includes(record.reference.value)))
          owner = candidate;
      }
    }
    let reference = record.reference;
    if (provider === "codex" && reference) {
      check(
        reference.kind === "id" && UUID.test(reference.value),
        "invalid Codex session reference",
      );
      if (
        !historyFiles(
          join(process.env.CODEX_HOME || join(home(), ".codex"), "sessions"),
          reference.value,
        ).length
      ) {
        if (owner && oldPane)
          check(
            !array(object(await backendRpc(oldPane, "thread/loaded/list", {})).data).includes(
              reference.value,
            ),
            "unpersisted session is still loaded",
          );
        reference = null;
        record = await patchSession(id, { reference: null });
      }
    }
    let pane: string | undefined;
    if (oldPane) {
      try {
        const p = object(
          (await api(binary, "pane.process_info", { pane_id: oldPane })).process_info,
        );
        if (p.shell_pid === p.foreground_process_group_id) pane = oldPane;
      } catch (error) {
        check(
          error instanceof Error && error.message.includes("not_found"),
          "pane state unavailable",
        );
      }
    }
    const name = `kite-${provider}-${id.slice(0, 8)}`;
    if (!pane) {
      const created = await api(binary, "workspace.create", {
        cwd: record.cwd ?? null,
        label: name,
        focus: false,
      });
      pane = string(object(created.root_pane).pane_id);
      record = await patchSession(id, {
        workspace_id: string(object(created.workspace).workspace_id),
        pane_id: pane,
        terminal_id: string(object(created.root_pane).terminal_id),
        agent_name: name,
      });
    } else record = await patchSession(id, { agent_name: name });
    let args: string[] = [],
      prepared: OwnerIdentity | undefined;
    if (provider === "codex") {
      let info: Registry;
      if (owner && owner.pane !== pane) {
        atomic(registryPath(pane), { ...owner, pane });
        info = readRegistry(pane);
      } else info = await ensure(pane, binary);
      await noteSessionOpen(pane);
      await reserveExisting(info);
      if (process.platform === "darwin" && !reference) {
        // The current Mac TUI initially shows its home screen without a thread.
        // Create the Kite-owned conversation before attaching the terminal;
        // explicit legacy history matches Kite's existing history contract.
        const thread = await createMacThread(pane, record.cwd ?? home(), record.title);
        reference = { source: "herdr:codex", agent: "codex", kind: "id", value: thread };
        record = await patchSession(id, { reference });
      }
      try {
        await backendRpc(pane, "account/read", { refreshToken: false });
      } catch {
        /* read-only warmup is optional */
      }
      args = [
        "--remote",
        `unix://${info.socket}`,
        "-c",
        "check_for_update_on_startup=false",
        ...(reference ? ["resume", reference.value] : []),
      ];
    } else {
      const p = object((await api(binary, "pane.process_info", { pane_id: pane })).process_info);
      check(p.shell_pid === p.foreground_process_group_id, "agent pane is not an available shell");
      prepared = await admit(pane, Number(p.shell_pid));
      if (reference) {
        const value =
          reference.kind === "id"
            ? reference.value
            : reference.kind === "path"
              ? basename(reference.value, ".jsonl")
              : "";
        check(/^[a-zA-Z0-9_-]+$/.test(value), "invalid Claude session ID");
        args = ["--resume", value];
      }
    }
    let started = object(
      (await api(binary, "agent.start", { pane_id: pane, name, kind: provider, args })).agent,
    );
    await patchSession(id, {
      terminal_id: string(started.terminal_id),
      pane_id: pane,
      workspace_id: string(started.workspace_id),
      agent_name: name,
    });
    for (let i = 0; i < 12; i++) {
      if (provider === "codex")
        try {
          await bind(pane, binary, reference?.value);
        } catch {
          /* TUI may still be bootstrapping */
        }
      const current = array((await api(binary, "agent.list", {})).agents)
        .map(object)
        .find((a) => a.terminal_id === started.terminal_id);
      if (current) {
        started = current;
        if (current.agent_session) {
          await patchSession(id, {
            reference: parseReference(current.agent_session),
          });
          if (prepared) await ready(prepared);
          return { agent: current };
        }
      }
      await sleep(200);
    }
    return { agent: started };
  });
}
export async function upgrade(pane: string, binary: string): Promise<Json> {
  const agent = array((await api(binary, "agent.list", {})).agents)
    .map(object)
    .find((a) => a.pane_id === pane);
  check(
    agent && agent.agent === "codex" && ["idle", "done"].includes(String(agent.agent_status)),
    "only an idle Codex pane can be upgraded",
  );
  const thread = object(agent.agent_session);
  check(
    thread.kind === "id" && UUID.test(string(thread.value)),
    "persisted Codex thread ID required",
  );
  const info = await ensure(pane, binary),
    p = object((await api(binary, "pane.process_info", { pane_id: pane })).process_info),
    front = array(p.foreground_processes).map(object);
  check(
    front.length === 1 &&
      front[0]?.name === "codex" &&
      array(front[0]?.argv).includes("--no-daemon"),
    "expected one idle in-process Codex TUI",
  );
  const pid = Number(front[0]?.pid),
    ticks = startTicks(pid);
  const fresh = array((await api(binary, "agent.list", {})).agents)
    .map(object)
    .find((a) => a.pane_id === pane);
  check(
    fresh &&
      ["idle", "done"].includes(String(fresh.agent_status)) &&
      JSON.stringify(fresh.agent_session) === JSON.stringify(thread) &&
      startTicks(pid) === ticks,
    "session became active; upgrade cancelled",
  );
  process.kill(pid, "SIGTERM");
  for (let i = 0; i < 100 && startTicks(pid); i++) await sleep(100);
  check(!startTicks(pid), "old session did not stop");
  await sleep(500);
  await api(binary, "agent.start", {
    pane_id: pane,
    name: agent.name,
    kind: "codex",
    args: [
      "--remote",
      `unix://${info.socket}`,
      "-c",
      "check_for_update_on_startup=false",
      "resume",
      string(thread.value),
    ],
  });
  for (let i = 0; i < 100; i++) {
    try {
      await bind(pane, binary, string(thread.value));
    } catch {
      /* wait for native TUI */
    }
    const current = array((await api(binary, "agent.list", {})).agents)
      .map(object)
      .find((a) => a.pane_id === pane);
    if (current?.interactive_ready && object(current.agent_session).value === thread.value)
      return { thread: thread.value, pane, protocol: 1 };
    await sleep(200);
  }
  throw new Error("TUI resume not confirmed; inspect pane, do not create another thread");
}
export function installResourceHooks(): void {
  setResourceHooks({
    catalog: async () => (await catalogRead()).sessions,
    readRegistry,
    registryPath,
    ownerLock,
    sessionLock,
    managed: async (binary, table) => {
      const result: [OwnerIdentity, ProcessFact[]][] = [],
        records = (await catalogRead()).sessions;
      const claude = records.some((r) => r.provider === "claude" && r.reference)
        ? array((await api(binary, "agent.list", {})).agents).map(object)
        : [];
      for (const record of records) {
        try {
          const pane = record.pane_id;
          if (record.provider === "dsh" || record.provider === "ply") {
            const ref = record.reference;
            if (ref?.source !== "kite:harness" || ref.agent !== record.provider) continue;
            const path = harnessDirectory(ref.value),
              state = object(readJson(join(path, "state.json"))),
              info = harnessInfo(readJson(join(path, "session.json"))),
              item = table.get(Number(state.pid));
            if (
              !item ||
              item.started !== state.start_ticks ||
              info.id !== record.id ||
              info.provider !== record.provider
            )
              continue;
            const argv = processArgv(item.pid);
            if (
              (!argv.includes("harness-owner") &&
                !argv.some((arg) => basename(arg).startsWith("kite_harness_runtime."))) ||
              !argv.includes(ref.value)
            )
              continue;
            result.push([
              {
                pane: `kite-harness-${ref.value}`,
                pid: item.pid,
                start_ticks: item.started,
                registry_managed: false,
              },
              descendants(table, [item.pid]),
            ]);
          } else if (pane && record.provider === "claude") {
            if (!claude.some((a) => sameReference(a, record))) continue;
            const p = object(
                (await api(binary, "pane.process_info", { pane_id: pane })).process_info,
              ),
              item = table.get(Number(p.shell_pid));
            if (item)
              result.push([
                {
                  pane,
                  pid: item.pid,
                  start_ticks: item.started,
                  shell_pid: item.pid,
                  registry_managed: false,
                },
                descendants(table, [item.pid]),
              ]);
          } else if (pane && record.provider === "codex") {
            const owner = readRegistry(pane),
              p = object((await api(binary, "pane.process_info", { pane_id: pane })).process_info);
            result.push([
              { ...owner, shell_pid: Number(p.shell_pid) },
              descendants(table, [owner.pid, Number(p.shell_pid)]),
            ]);
          }
        } catch {
          /* ambiguous owner is not adopted */
        }
      }
      return result;
    },
  });
}
