import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readJson } from "@kite/runtime";
import { sessionEnvironment } from "./desktop";
import { payload } from "./push";
import {
  descendants,
  insideBudget,
  ledger,
  liveLegacy,
  type ProcessFact,
  resourceRoot,
  scopeName,
  userManagerCgroup,
} from "./resources";

let root = "",
  home: string | undefined;
beforeEach(() => {
  home = process.env.HOME;
  root = mkdtempSync(join(tmpdir(), "kite-platform-test-"));
  process.env.HOME = root;
});
afterEach(() => {
  if (home === undefined) delete process.env.HOME;
  else process.env.HOME = home;
  rmSync(root, { recursive: true, force: true });
});
const proc = (pid: number, ppid: number, started = String(pid)): ProcessFact => ({
  pid,
  ppid,
  started,
  uid: 1000,
  name: "owned",
  ticks: 0,
  rss: 1,
  reserved: 2,
  cgroup: "/user.slice/kite-agents.slice/owned.scope",
});
test("resource adoption follows every descendant and PID start identity", () => {
  const table = new Map([
    [2, proc(2, 1)],
    [3, proc(3, 2)],
    [4, proc(4, 3)],
    [5, proc(5, 1)],
  ]);
  expect(descendants(table, [2]).map((p) => p.pid)).toEqual([2, 3, 4]);
  expect(insideBudget(proc(2, 1))).toBe(true);
  expect(
    liveLegacy(
      {
        version: 1,
        units: {},
        legacy: {
          "2:old": { pid: 2, started: "old", reserved: 20 },
          "3:3": { pid: 3, started: "3", reserved: 10 },
        },
      },
      table,
    ),
  ).toEqual({ "3:3": { pid: 3, started: "3", reserved: 10 } });
  expect(scopeName("owned", 2, "300")).toBe("kite-agent-2-300.scope");
});
test("invalid memory ledgers are refused and preserved", () => {
  const file = join(resourceRoot(), "budget.json"),
    raw = JSON.stringify({
      version: 1,
      legacy: { bad: { pid: 2, started: "3", reserved: 10 } },
      units: {},
    });
  writeFileSync(file, raw);
  expect(() => ledger()).toThrow("invalid legacy");
  expect(readFileSync(file, "utf8")).toBe(raw);
});
test("credentials require private non-symlink files", () => {
  const file = join(root, "account.json");
  writeFileSync(file, '{"owned":true}', { mode: 0o600 });
  expect(readJson(file, 65536, true)).toEqual({ owned: true });
  chmodSync(file, 0o644);
  expect(() => readJson(file, 65536, true)).toThrow();
  chmodSync(file, 0o600);
  symlinkSync(file, join(root, "link"));
  expect(() => readJson(join(root, "link"), 65536, true)).toThrow();
});
test("push contains only bounded event identity and validates event kinds", () => {
  const body = payload(
    { host_id: "owned-host", token: "fixture-token" },
    { id: "a".repeat(64), kind: "completed", session_id: "owned-session", created_ms: 123 },
  );
  expect(JSON.stringify(body)).toContain("owned-host/");
  expect(JSON.stringify(body)).not.toContain("prompt");
  expect(() =>
    payload(
      { host_id: "../escape", token: "fixture-token" },
      { id: "a".repeat(64), kind: "completed" },
    ),
  ).toThrow();
  expect(() => payload({ host_id: "owned" }, { id: "a".repeat(64), kind: "invented" })).toThrow();
});
test("desktop environment removes supervisor and agent identity variables", () => {
  const previous = { ...process.env };
  try {
    process.env.DISPLAY = ":98765";
    delete process.env.XDG_SESSION_TYPE;
    process.env.HERDR_PANE_ID = "owned-pane";
    process.env.NOTIFY_SOCKET = "owned-notify";
    process.env.CODEX_THREAD_ID = "owned-thread";
    process.env.CHROME_USER_DATA_DIR = "/owned/profile";
    const env = sessionEnvironment();
    expect(env.HERDR_PANE_ID).toBeUndefined();
    expect(env.NOTIFY_SOCKET).toBeUndefined();
    expect(env.CODEX_THREAD_ID).toBeUndefined();
    expect(env.CHROME_USER_DATA_DIR).toBeUndefined();
  } finally {
    for (const k of Object.keys(process.env)) if (!(k in previous)) delete process.env[k];
    Object.assign(process.env, previous);
  }
});

test("sampling uses the user manager outside the app slice", async () => {
  const manager = "/user.slice/user-1000.slice/user@1000.service";
  let queries = 0;
  const lookup = async () => {
    queries++;
    return manager;
  };
  expect(await userManagerCgroup(`${manager}/app.slice/kite-node.service`, lookup)).toBe(manager);
  expect(queries).toBe(0);
  expect(
    await userManagerCgroup(`${manager}/kite.slice/kite-agents.slice/kite-agent-1-2.scope`, lookup),
  ).toBe(manager);
  expect(await userManagerCgroup(undefined, lookup)).toBe(manager);
  expect(queries).toBe(2);
});
