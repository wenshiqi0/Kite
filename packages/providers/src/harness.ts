import { closeSync, existsSync, openSync, readFileSync } from "node:fs";
import { basename, isAbsolute, join } from "node:path";
import { budgetCommand, ready, scopeName } from "@kite/platform";
import {
  array,
  check,
  type Json,
  json,
  type ManagedProvider,
  type Model,
  type ObjectValue,
  object,
  optionalString,
  parse,
  type SessionRecord,
  string,
  UUID,
} from "@kite/protocol";
import {
  atomic,
  home,
  privateDir,
  readJson,
  selfCommand,
  sha256,
  sleep,
  startTicks,
} from "@kite/runtime";
import { listening, nativeModelId, requestJson } from "@kite/runtime/socket";
import { connectNative } from "./native";
export const harnessRoot = () => {
  const path = process.env.KITE_HARNESS_STATE_DIR || join(home(), ".local/state/kite/harnesses");
  check(isAbsolute(path), "harness state directory must be absolute");
  return privateDir(path);
};
export const harnessDirectory = (thread: string) => {
  check(UUID.test(thread), "invalid harness reference");
  return join(harnessRoot(), thread);
};
export interface HarnessInfo {
  id: string;
  thread: string;
  provider: ManagedProvider;
  route: string;
  cwd: string;
  title: string;
  command: string[];
  model: string;
}
export function harnessInfo(value: unknown): HarnessInfo {
  const r = object(value);
  check(r.provider === "dsh" || r.provider === "ply", "invalid harness provider");
  return {
    id: string(r.id),
    thread: string(r.thread),
    provider: r.provider,
    route: optionalString(r.route),
    cwd: string(r.cwd),
    title: optionalString(r.title),
    command: array(r.command).map(string),
    model: string(r.model),
  };
}
export function configuration(provider: ManagedProvider): {
  command: string[];
  model: string;
  route: string;
} {
  const prefix = `KITE_${provider.toUpperCase()}`;
  check(process.env[`KITE_ENABLE_${provider.toUpperCase()}`] === "1", "harness is not enabled");
  const model = process.env[`${prefix}_MODEL`] || "deepseek-v4-flash",
    route =
      process.env[`${prefix}_PROVIDER`] || (provider === "dsh" ? "deepseek-official" : "deepseek");
  const override = process.env[`${prefix}_COMMAND`];
  let command = override
    ? array(parse(override)).map(string)
    : provider === "dsh"
      ? ["dsh", "--profile", "acp"]
      : [
          "ply",
          "mcp",
          "--host-chat",
          "--approval-mode",
          "ask",
          "--model",
          model,
          ...(process.env.KITE_PLY_PROVIDER ? ["--mimo-provider", route] : []),
        ];
  check(
    command.length > 0 &&
      command.length <= 24 &&
      command.every((s) => s.length > 0 && !s.includes("\0")),
    "invalid harness command",
  );
  const executable = Bun.which(string(command[0]));
  check(executable, "harness executable unavailable");
  command = [executable, ...command.slice(1)];
  return { command, model, route };
}
export async function modelIds(provider: ManagedProvider, model: string): Promise<Set<string>> {
  const prefix = `KITE_${provider.toUpperCase()}`,
    key = process.env[model.startsWith("mimo-") ? "MIMO_API_KEY" : "DEEPSEEK_API_KEY"];
  check(key, "harness model credential unavailable");
  let endpoint = process.env[`${prefix}_HEALTH_URL`];
  if (!endpoint) {
    check(
      ["deepseek-official", "deepseek"].includes(process.env[`${prefix}_PROVIDER`] || "deepseek"),
      "custom model routes require a matching health URL",
    );
    endpoint = "https://api.deepseek.com/v1/models";
  }
  const url = new URL(endpoint);
  check(
    !url.username &&
      !url.password &&
      (url.protocol === "https:" ||
        (url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))),
    "invalid harness health URL",
  );
  const rows = object(
    await requestJson(endpoint, {
      headers: {
        Authorization: `Bearer ${key}`,
        ...(model.startsWith("mimo-") ? { "api-key": key } : {}),
      },
    }),
  );
  const ids = new Set(
    array(rows.data)
      .map(object)
      .map((r) => string(r.id)),
  );
  check(ids.has(model), "harness model unavailable");
  return ids;
}
export async function probe(provider: ManagedProvider): Promise<Model[]> {
  const conf = configuration(provider),
    available = await modelIds(provider, conf.model),
    native = await connectNative(provider, {
      ...conf,
      cwd: home(),
      inspect: true,
      event: () => {},
      permission: () => {},
      persist: () => {},
    });
  try {
    return native.models.filter((r) => available.has(nativeModelId(r.model)));
  } finally {
    native.close();
  }
}
export async function providers(): Promise<Json[]> {
  const values = Object.fromEntries(
    Object.entries(process.env).filter(
      ([key]) =>
        key.startsWith("KITE_") ||
        ["PATH", "DEEPSEEK_API_KEY", "MIMO_API_KEY", "DSH_HOME"].includes(key),
    ),
  );
  const file = join(
    harnessRoot(),
    `probe-${sha256(JSON.stringify(Object.entries(values).sort()))}.json`,
  );
  try {
    const value = object(readJson(file));
    if (Date.now() - Number(value.at) < 20000) return array(value.rows);
  } catch {
    /* probe current configuration */
  }
  const rows = await Promise.all(
    (["codex", "claude", "dsh", "ply"] as const).map(async (p) => {
      try {
        if (p === "dsh" || p === "ply") {
          if (!(await probe(p)).length) return null;
        } else if (!Bun.which(p)) return null;
        return {
          id: p,
          managed_chat: p !== "claude",
          models: p !== "claude",
          attachments: p !== "claude",
          skills: p === "codex",
          usage: p === "codex",
          pause: p === "codex" || p === "claude",
          terminal: p === "codex" || p === "claude",
        };
      } catch {
        return null;
      }
    }),
  );
  const result = rows.filter((r) => r !== null);
  atomic(file, { at: Date.now(), rows: result });
  return result;
}
export function harnessAgent(info: HarnessInfo, state: ObjectValue): ObjectValue {
  const terminal = `kite-harness-${info.thread}`;
  return {
    terminal_id: terminal,
    pane_id: terminal,
    tab_id: terminal,
    workspace_id: terminal,
    name: info.title,
    title: null,
    agent: info.provider,
    cwd: info.cwd,
    agent_status: state.status ?? "idle",
    revision: state.revision ?? 1,
    state_change_seq: state.revision ?? 1,
    launch_pending: false,
    interactive_ready: true,
    agent_session: { source: "kite:harness", agent: info.provider, kind: "id", value: info.thread },
  };
}
export async function harnessAgents(records: SessionRecord[]): Promise<ObjectValue[]> {
  const result: ObjectValue[] = [];
  for (const r of records) {
    const ref = r.reference;
    if (ref?.source !== "kite:harness") continue;
    try {
      const path = harnessDirectory(ref.value);
      if (!(await listening(join(path, "owner.sock")))) continue;
      result.push(
        harnessAgent(
          harnessInfo(readJson(join(path, "session.json"))),
          object(readJson(join(path, "state.json"))),
        ),
      );
    } catch {
      /* saved owner is unavailable */
    }
  }
  return result;
}
export async function openHarness(record: SessionRecord): Promise<ObjectValue> {
  check(record.provider === "dsh" || record.provider === "ply", "unsupported managed provider");
  const conf = configuration(record.provider),
    thread = record.reference?.value || crypto.randomUUID(),
    path = privateDir(harnessDirectory(thread));
  if (await listening(join(path, "owner.sock"))) {
    const info = harnessInfo(readJson(join(path, "session.json")));
    check(info.id === record.id && info.provider === record.provider, "harness identity mismatch");
    const state = object(readJson(join(path, "state.json")));
    if (state.protocol === 2) return harnessAgent(info, state);
    check(
      !["working", "blocked"].includes(String(state.status)),
      "legacy harness turn is active; finish it before migration",
    );
    const pid = Number(state.pid),
      ticks = string(state.start_ticks);
    check(startTicks(pid) === ticks, "harness owner identity changed");
    const argv = readFileSync(`/proc/${pid}/cmdline`).toString().split("\0");
    check(
      argv.includes(thread) &&
        argv.some((arg) => basename(arg).startsWith("kite_harness_runtime.")),
      "legacy owner identity mismatch",
    );
    process.kill(pid, "SIGTERM");
    for (let i = 0; i < 80 && startTicks(pid) === ticks; i++) await sleep(50);
    check(startTicks(pid) !== ticks, "legacy owner did not stop");
  }
  if (existsSync(join(path, "state.json"))) {
    const old = object(readJson(join(path, "state.json")));
    check(
      typeof old.pid !== "number" || startTicks(old.pid) !== old.start_ticks,
      "existing harness owner is unavailable; inspect it before restoring",
    );
    check(
      !["working", "blocked"].includes(String(old.status)),
      "harness owner was interrupted; inspect history before restoring",
    );
  }
  const info: HarnessInfo = {
    id: record.id,
    thread,
    provider: record.provider,
    cwd: record.cwd || home(),
    title: record.title,
    ...conf,
  };
  atomic(join(path, "session.json"), info);
  let command = selfCommand("harness-owner", thread);
  const budgeted = process.env.KITE_AGENT_RESOURCE_CONTROL === "1";
  if (budgeted) command = budgetCommand(`kite-harness-${thread}`, command);
  const log = openSync(join(path, "owner.log"), "a", 0o600);
  const child = Bun.spawn(command, {
    stdin: "ignore",
    stdout: "ignore",
    stderr: log,
    detached: true,
  });
  closeSync(log);
  child.unref();
  for (let i = 0; i < 300; i++) {
    check(child.exitCode === null, "harness owner initialization failed");
    if (await listening(join(path, "owner.sock"))) {
      if (budgeted) {
        const ticks = startTicks(child.pid);
        check(ticks, "harness owner exited");
        await ready({
          pane: `kite-harness-${thread}`,
          pid: child.pid,
          start_ticks: ticks,
          budget_unit: scopeName(`kite-harness-${thread}`, child.pid, ticks),
        });
      }
      return harnessAgent(info, object(readJson(join(path, "state.json"))));
    }
    await sleep(100);
  }
  throw new Error("harness owner initialization timed out");
}
export function migrationContext(path: string): string | undefined {
  const file = join(path, "history.jsonl");
  if (!existsSync(file)) return undefined;
  const raw = readFileSync(file);
  check(raw.length <= 2 * 1024 * 1024, "migration context exceeds limit");
  const previous = raw
    .toString()
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => object(parse(line)).payload);
  const result = JSON.stringify(previous);
  check(Buffer.byteLength(result) <= 512 * 1024, "migration context exceeds limit");
  return result;
}
export const infoJson = (info: HarnessInfo): Json => json(info);
