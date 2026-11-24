import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { gunzipSync } from "node:zlib";
import { array, check, type Json, json, object, parse, string } from "@kite/protocol";
import {
  bounded,
  lines,
  live,
  privateDir,
  sha256,
  sleep,
  startTicks,
  tryLock,
} from "@kite/runtime";
import { RpcProcess } from "@kite/runtime/rpc";
import { listening } from "@kite/runtime/socket";
import { AcpRpc } from "../src/acp";

const root = mkdtempSync(join(tmpdir(), "kite-ts-test-")),
  cli = resolve("target/bun/host.js"),
  fixture = resolve("packages/host/test/fixtures/native.ts");
beforeAll(async () => {
  const build = Bun.spawn([process.execPath, "run", "scripts/build-host.ts"], {
    stdout: "ignore",
    stderr: "pipe",
  });
  check((await build.exited) === 0, await new Response(build.stderr).text());
});
afterAll(() => {
  if (!process.env.KITE_KEEP_FIXTURES) rmSync(root, { recursive: true, force: true });
  else console.error(root);
});
const env = (home: string, extra: NodeJS.ProcessEnv = {}) => ({
  ...process.env,
  KITE_HOST_BACKEND_PATH: cli,
  HOME: home,
  CODEX_HOME: join(home, ".codex"),
  CODEX_THREAD_ID: undefined,
  KITE_SESSION_ID: undefined,
  KITE_AGENT_RESOURCE_CONTROL: "0",
  KITE_ENABLE_DSH: "0",
  KITE_ENABLE_PLY: "0",
  KITE_ENABLE_ANTIGRAVITY: "0",
  KITE_HARNESS_STATE_DIR: join(home, "harnesses"),
  ...extra,
});
async function command(
  home: string,
  args: string[],
  data?: string | Buffer,
  extra: NodeJS.ProcessEnv = {},
): Promise<{ code: number; stdout: Buffer; stderr: Buffer }> {
  const child = Bun.spawn([process.execPath, cli, ...args], {
    env: env(home, extra),
    stdin: data ? Buffer.from(data) : "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    bounded(child.stdout, 40 * 1024 * 1024),
    bounded(child.stderr, 65536),
  ]);
  return { code, stdout, stderr };
}
const fresh = () => privateDir(join(root, crypto.randomUUID()));
const row = (id = "a".repeat(32), provider = "ply") => ({
  id,
  provider,
  title: "Owned fixture",
  reference: null,
  cwd: root,
});
describe("catalog and owned upload CLI", () => {
  test("the CLI deletes drafts idempotently and fences stale catalog writes", async () => {
    const home = fresh();
    expect(
      (await command(home, ["catalog-write"], JSON.stringify({ revision: 0, sessions: [row()] })))
        .code,
    ).toBe(0);
    const args = ["session-delete", "--id", "a".repeat(32), "--herdr", "unavailable"];
    const deleted = await command(home, args);
    expect(deleted.code).toBe(0);
    expect(object(parse(deleted.stdout.toString())).sessions).toEqual([]);
    expect((await command(home, args)).code).toBe(0);
    const stale = await command(
      home,
      ["catalog-write"],
      JSON.stringify({ revision: 1, sessions: [row()] }),
    );
    expect(object(parse(stale.stdout.toString())).conflict).toBe(true);
    const freshWrite = await command(
      home,
      ["catalog-write"],
      JSON.stringify({ revision: 2, sessions: [row()] }),
    );
    expect(object(parse(freshWrite.stdout.toString())).sessions).toEqual([]);
  });
  test("accepts the Rust Catalog wire DTO without an on-disk version field", async () => {
    const home = fresh();
    const result = await command(
      home,
      ["catalog-write"],
      JSON.stringify({ revision: 0, now_ms: 1, sessions: [row()] }),
    );
    expect(result.code).toBe(0);
    const saved = object(parse(result.stdout.toString()));
    expect(saved.version).toBe(1);
    expect(saved.revision).toBe(1);
    expect(array(saved.sessions)).toHaveLength(1);
  });
  test("returns the upload receipt while the client input stream remains open", async () => {
    const home = fresh(),
      data = Buffer.from("owned upload with an open input stream"),
      id = "e".repeat(32);
    const child = Bun.spawn(
      [
        process.execPath,
        cli,
        "upload",
        "--id",
        id,
        "--name",
        "owned.bin",
        "--size",
        String(data.length),
        "--sha256",
        sha256(data),
      ],
      { env: env(home), stdin: "pipe", stdout: "pipe", stderr: "pipe" },
    );
    const reader = child.stdout.getReader();
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      child.stdin.write(data);
      await child.stdin.flush();
      const response = await Promise.race([
        reader.read(),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error("receipt waited for EOF")), 2000);
        }),
      ]);
      clearTimeout(timer);
      expect(response.done).toBe(false);
      const receipt = object(
        parse(
          Buffer.from(response.value || [])
            .toString()
            .trim(),
        ),
      );
      expect(receipt.sha256).toBe(sha256(data));
      expect(receipt.size).toBe(data.length);
      child.stdin.end();
      expect(await child.exited).toBe(0);
    } finally {
      clearTimeout(timer);
      reader.releaseLock();
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
  test("concurrent revision writes have one winner and preserve extra session fields", async () => {
    const home = fresh(),
      value = { version: 1, revision: 0, sessions: [{ ...row(), preview: "retained" }] };
    const results = await Promise.all(
      Array.from({ length: 8 }, () => command(home, ["catalog-write"], JSON.stringify(value))),
    );
    expect(results.every((r) => r.code === 0)).toBe(true);
    expect(results.filter((r) => !object(parse(r.stdout.toString())).conflict)).toHaveLength(1);
    const read = object(parse((await command(home, ["catalog-read"])).stdout.toString()));
    expect(read.revision).toBe(1);
    expect(object(array(read.sessions)[0]).preview).toBe("retained");
  });
  test("validates catalog and bounds untrusted input", async () => {
    const home = fresh();
    for (const value of [
      { version: 2, revision: 0, sessions: [] },
      { version: 1, revision: -1, sessions: [] },
      { version: 1, revision: 0, sessions: [{ id: "../escape", provider: "ply", title: "bad" }] },
    ])
      expect((await command(home, ["catalog-write"], JSON.stringify(value))).code).toBe(1);
    expect((await command(home, ["catalog-write"], "x".repeat(1048577))).code).toBe(1);
  });
  test("checksums uploads and refuses overwrite, traversal and symlinks", async () => {
    const home = fresh(),
      data = Buffer.from("owned binary data\0"),
      id = "b".repeat(32),
      args = [
        "upload",
        "--id",
        id,
        "--name",
        "image.bin",
        "--size",
        String(data.length),
        "--sha256",
        sha256(data),
      ];
    expect((await command(home, args, data)).code).toBe(0);
    expect((await command(home, args, data)).code).toBe(1);
    const result = await command(home, ["attachment-read", "--id", id, "--name", "image.bin"]);
    expect(result.code).toBe(0);
    const at = result.stdout.indexOf(10);
    expect(result.stdout.subarray(at + 1)).toEqual(data);
    expect(statSync(join(home, ".local/share/kite/uploads", id, "image.bin")).mode & 0o777).toBe(
      0o600,
    );
    expect(
      (await command(home, [...args.slice(0, 4), "../escape", ...args.slice(5)], data)).code,
    ).toBe(1);
    const bad = "c".repeat(32),
      dir = privateDir(join(home, ".local/share/kite/uploads", bad));
    symlinkSync(join(root, "outside"), join(dir, "image.bin"));
    expect(
      (await command(home, ["attachment-read", "--id", bad, "--name", "image.bin"])).code,
    ).toBe(1);
    expect(
      (
        await command(
          home,
          [
            "upload",
            "--id",
            "d".repeat(32),
            "--name",
            "bad",
            "--size",
            "4",
            "--sha256",
            "0".repeat(64),
          ],
          Buffer.from("test"),
        )
      ).code,
    ).toBe(1);
    expect(existsSync(join(home, ".local/share/kite/uploads", "d".repeat(32)))).toBe(false);
  });
});

test("discovery keeps the parent owner when a separate Codex child reports its inherited pane", async () => {
  const home = fresh(),
    pane = "owned:p1",
    parent = crypto.randomUUID(),
    childThread = crypto.randomUUID();
  const socket = join(home, "discovery.sock"),
    requests = join(home, "owner-requests");
  const owner = Bun.spawn(
    [
      process.execPath,
      resolve("packages/host/test/fixtures/discovery-owner.ts"),
      socket,
      parent,
      requests,
      "app-server",
      `unix://${socket}`,
    ],
    {
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  let bridge: ReturnType<typeof Bun.spawn> | undefined;
  try {
    const reader = owner.stdout.getReader();
    expect((await reader.read()).done).toBe(false);
    reader.releaseLock();
    const registry = privateDir(join(home, ".local/state/kite/codex"));
    writeFileSync(
      join(registry, `${sha256(pane).slice(0, 20)}.json`),
      JSON.stringify({
        protocol: 1,
        pane,
        pid: owner.pid,
        start_ticks: startTicks(owner.pid),
        socket,
      }),
    );
    const herdr = join(home, "herdr.ts"),
      nativeRequests = join(home, "native-requests");
    writeFileSync(
      herdr,
      `#!${process.execPath}
import {appendFileSync} from "node:fs";
const request = JSON.parse(await Bun.stdin.text());
appendFileSync(${JSON.stringify(nativeRequests)}, JSON.stringify(request)+"\\n");
let result;
if(request.method === "agent.list") result={type:"agent_list",agents:[{
  agent:"codex",terminal_id:"parent-terminal",pane_id:${JSON.stringify(pane)},agent_status:"working",interactive_ready:true,
  agent_session:{source:"herdr:codex",agent:"codex",kind:"id",value:${JSON.stringify(childThread)}}}]};
else if(request.method === "agent.prompt") result={type:"agent_prompted",marker:request.params.prompt};
else throw new Error("unexpected mutation");
console.log(JSON.stringify({id:request.id,result}));
`,
    );
    chmodSync(herdr, 0o700);
    // SSH clients wait for a response before closing stdin.
    const bridgeProcess = Bun.spawn([process.execPath, cli, "herdr-api", "--herdr", herdr], {
      env: env(home),
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    bridge = bridgeProcess;
    const timer = setTimeout(() => bridgeProcess.kill(), 3000);
    try {
      bridgeProcess.stdin.write(
        `${JSON.stringify({ id: 23, method: "agent.list", params: {} })}\n`,
      );
      await bridgeProcess.stdin.flush();
      const response = object(parse(await new Response(bridgeProcess.stdout).text()));
      const agent = object(array(object(response.result).agents)[0]);
      expect(response.id).toBe(23);
      expect(object(agent.agent_session).value).toBe(parent);
      expect(agent.terminal_id).toBe("parent-terminal");
      expect(agent.interactive_ready).toBe(true);
      expect(agent.subagents_error).toBe(false);
      expect(array(agent.subagents)).toHaveLength(1);
      expect(object(array(agent.subagents)[0])).toMatchObject({
        parent_id: parent,
        name: "Worker",
        runtime_status: "active",
      });
      expect(await bridgeProcess.exited).toBe(0);
      bridgeProcess.stdin.end();
    } finally {
      clearTimeout(timer);
    }
    const prompt = {
      id: "owned-followup",
      method: "agent.prompt",
      params: { prompt: "owned marker" },
    };
    const forwarded = await command(
      home,
      ["herdr-api", "--herdr", herdr],
      `${JSON.stringify(prompt)}\n`,
    );
    expect(object(parse(forwarded.stdout.toString()))).toEqual({
      id: prompt.id,
      result: { type: "agent_prompted", marker: "owned marker" },
    });
    expect(
      readFileSync(nativeRequests, "utf8")
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line).method),
    ).toEqual(["agent.list", "agent.prompt"]);
    expect(readFileSync(requests, "utf8").trim().split("\n")).toEqual([
      "initialize",
      "thread/loaded/list",
      "initialize",
      "thread/list",
      "initialize",
      "thread/list",
    ]);
    expect(owner.exitCode).toBeNull();
  } finally {
    if (bridge && bridge.exitCode === null) bridge.kill();
    owner.kill();
    await owner.exited;
  }
});

describe("archived Codex owner reclamation", () => {
  const cases: {
    name: string;
    release: boolean;
    loaded?: boolean;
    status?: string;
    ready?: boolean;
    history?: boolean;
    archivedHistory?: boolean;
    alias?: "archived" | "active";
    paused?: boolean;
    foreign?: boolean;
    background?: "idle" | "active";
    backgroundHistory?: boolean;
    emptyHistory?: boolean;
  }[] = [
    { name: "a completed TUI without an interactive readiness flag", release: true },
    {
      name: "only archived aliases of the same owner",
      alias: "archived",
      ready: true,
      release: true,
    },
    { name: "an unloaded thread with saved context", loaded: false, ready: true, release: true },
    {
      name: "an unloaded thread with natively archived context",
      loaded: false,
      ready: true,
      archivedHistory: true,
      release: true,
    },
    { name: "an unarchived alias", alias: "active", release: false },
    { name: "an active native thread", status: "active", ready: true, release: false },
    { name: "an unknown native status", status: "notLoaded", ready: true, release: false },
    {
      name: "an unloaded thread without saved context",
      loaded: false,
      history: false,
      release: false,
    },
    { name: "a paused TUI", paused: true, ready: true, release: false },
    { name: "an unrelated foreground process", foreign: true, release: false },
    { name: "an active background thread", background: "active", ready: true, release: false },
    { name: "all idle, persisted native threads", background: "idle", ready: true, release: true },
    {
      name: "a background thread without saved context",
      background: "idle",
      backgroundHistory: false,
      ready: true,
      release: false,
    },
    {
      name: "an unloaded thread with an empty history",
      loaded: false,
      emptyHistory: true,
      release: false,
    },
  ];
  for (const mode of cases)
    test(`${mode.release ? "releases" : "retains"} ${mode.name}`, async () => {
      const home = fresh(),
        pane = "owned:p1",
        socket = join(home, "lifecycle.sock"),
        state = join(home, "native-state.json"),
        thread = crypto.randomUUID(),
        background = crypto.randomUUID(),
        history = join(
          privateDir(join(home, ".codex", mode.archivedHistory ? "archived_sessions" : "sessions")),
          `rollout-2026-11-17T02-00-00-${thread}.jsonl`,
        ),
        context = mode.emptyHistory ? "" : "durable original provider context\n",
        record = {
          ...row("a".repeat(32), "codex"),
          pane_id: pane,
          archived: true,
          reference: { source: "herdr:codex", agent: "codex", kind: "id", value: thread },
        };
      if (mode.history !== false) writeFileSync(history, context);
      const backgroundHistory = join(home, ".codex/sessions", `rollout-owned-${background}.jsonl`);
      if (mode.background && mode.backgroundHistory !== false)
        writeFileSync(backgroundHistory, "durable background context\n");
      writeFileSync(
        state,
        JSON.stringify({
          threads: mode.loaded === false ? [] : mode.background ? [thread, background] : [thread],
          status: mode.status || "idle",
          background: mode.background ? { [background]: mode.background } : {},
        }),
      );
      const owner = Bun.spawn(
          [
            process.execPath,
            resolve("packages/host/test/fixtures/lifecycle-owner.ts"),
            socket,
            state,
            "app-server",
            `unix://${socket}`,
          ],
          { stdin: "ignore", stdout: "pipe", stderr: "pipe", detached: true },
        ),
        tui = Bun.spawn([process.execPath, "-e", "setInterval(()=>{},1000)"], {
          stdin: "ignore",
          stdout: "ignore",
          stderr: "ignore",
        });
      const tuiTicks = startTicks(tui.pid),
        ownerTicks = startTicks(owner.pid);
      try {
        const reader = owner.stdout.getReader();
        expect((await reader.read()).done).toBe(false);
        reader.releaseLock();
        const registry = join(
          privateDir(join(home, ".local/state/kite/codex")),
          `${sha256(pane).slice(0, 20)}.json`,
        );
        writeFileSync(
          registry,
          JSON.stringify({ protocol: 1, pane, pid: owner.pid, start_ticks: ownerTicks, socket }),
        );
        const herdr = join(home, "herdr.ts");
        writeFileSync(
          herdr,
          `#!${process.execPath}
import { readFileSync } from "node:fs";
const request = JSON.parse(await Bun.stdin.text());
let alive = false;
try { alive = !/\\) [ZX] /.test(readFileSync("/proc/${tui.pid}/stat", "utf8")); } catch {}
let result;
if (request.method === "agent.list") result = { agents: alive ? [{
  agent: "codex", pane_id: ${JSON.stringify(pane)}, agent_status: "done",
  ${mode.ready === undefined ? "" : `interactive_ready: ${mode.ready},`}
  agent_session: ${JSON.stringify(record.reference)}
}] : [] };
else if (request.method === "pane.process_info") result = { process_info: {
  shell_pid: 424242, foreground_process_group_id: alive ? ${tui.pid} : 424242,
  foreground_processes: alive ? [{ pid: ${tui.pid}, name: ${JSON.stringify(mode.foreign ? "bash" : "codex")},
    argv: ["codex", "--remote", ${JSON.stringify(`unix://${socket}`)}] }] : []
} };
else throw new Error("unexpected mutation: " + request.method);
console.log(JSON.stringify({ id: request.id, result }));
`,
        );
        chmodSync(herdr, 0o700);
        const records = [record];
        if (mode.alias)
          records.push({
            ...record,
            id: "b".repeat(32),
            archived: mode.alias === "archived",
            reference: { ...record.reference, value: crypto.randomUUID() },
          });
        expect(
          (
            await command(
              home,
              ["catalog-write"],
              JSON.stringify({ revision: 0, sessions: records }),
            )
          ).code,
        ).toBe(0);
        if (mode.paused) process.kill(tui.pid, "SIGSTOP");
        const snapshot = await command(home, ["lifecycle-facts", "--herdr", herdr]);
        expect(snapshot.code).toBe(0);
        const fact = array(object(parse(snapshot.stdout.toString())).facts)
          .map(object)
          .find((f) => f.session_id === record.id);
        expect(fact).toBeDefined();
        if (mode.release) expect(fact).toMatchObject({ idle: true, persisted: true });
        const result = await command(
          home,
          ["lifecycle-reap", "--herdr", herdr],
          JSON.stringify({ ...fact, reason: "archived" }),
        );
        expect(result.code).toBe(0);
        expect(object(parse(result.stdout.toString()))).toMatchObject({ reclaimed: mode.release });
        expect(live(owner.pid, ownerTicks || "")).toBe(!mode.release);
        expect(live(tui.pid, tuiTicks || "")).toBe(!mode.release);
        expect(existsSync(registry)).toBe(!mode.release);
        expect(
          JSON.parse(readFileSync(join(home, ".local/share/kite/sessions.json"), "utf8")).sessions,
        ).toEqual(records);
        if (mode.history !== false) expect(readFileSync(history, "utf8")).toBe(context);
        if (mode.background && mode.backgroundHistory !== false)
          expect(readFileSync(backgroundHistory, "utf8")).toBe("durable background context\n");
      } finally {
        if (mode.paused && live(tui.pid, tuiTicks || "")) process.kill(tui.pid, "SIGCONT");
        if (tui.exitCode === null) tui.kill();
        if (owner.exitCode === null) owner.kill();
        await Promise.all([owner.exited, tui.exited]);
      }
    });
});

describe("explicit owner termination", () => {
  for (const mode of [
    "bound",
    "unbound",
    "shared",
    "foreign-terminal",
    "detached",
    "detached-alias",
    "shared-pane",
    "unknown-thread",
    "reloaded-thread",
  ])
    test(mode === "foreign-terminal"
      ? "an unrelated terminal foreground is never signalled"
      : mode === "shared"
        ? "a shared loaded backend and its tools are retained"
        : [
              "detached",
              "detached-alias",
              "shared-pane",
              "unknown-thread",
              "reloaded-thread",
            ].includes(mode)
          ? `deletion of a reused pane verifies native thread ownership: ${mode}`
          : `deletion stops a real ${mode} owner and its stubborn tool before removing the record`, async () => {
      const shared = mode === "shared",
        detached = mode === "detached" || mode === "detached-alias",
        reused = detached || ["shared-pane", "unknown-thread", "reloaded-thread"].includes(mode),
        retained = shared || mode === "foreign-terminal" || reused,
        rejected = retained && !detached;
      const home = fresh(),
        path = privateDir(join(home, "owner")),
        socket = join(path, "backend.sock"),
        thread = crypto.randomUUID(),
        peerThread = crypto.randomUUID(),
        pane = "owned:p1";
      const owner = Bun.spawn(
        [
          process.execPath,
          resolve("packages/host/test/fixtures/deletion-owner.ts"),
          socket,
          JSON.stringify(
            shared || mode === "shared-pane"
              ? [thread, peerThread]
              : mode === "unknown-thread"
                ? [crypto.randomUUID()]
                : reused
                  ? [peerThread]
                  : [thread],
          ),
          path,
          "app-server",
          `unix://${socket}`,
          mode === "reloaded-thread" ? "idle" : "",
        ],
        { detached: true, stdin: "ignore", stdout: "pipe", stderr: "pipe" },
      );
      let toolPid = 0,
        toolTicks: string | undefined;
      try {
        const reader = owner.stdout.getReader();
        expect((await reader.read()).done).toBe(false);
        reader.releaseLock();
        toolPid = Number(object(parse(readFileSync(join(path, "tool.json"), "utf8"))).pid);
        toolTicks = startTicks(toolPid);
        const ticks = startTicks(owner.pid);
        check(ticks && toolTicks, "owned process identity unavailable");
        const registry = privateDir(join(home, ".local/state/kite/codex"));
        writeFileSync(
          join(registry, `${sha256(pane).slice(0, 20)}.json`),
          JSON.stringify({ protocol: 1, pane, pid: owner.pid, start_ticks: ticks, socket }),
        );
        if (mode === "detached-alias")
          writeFileSync(
            join(registry, `${sha256("other:p1").slice(0, 20)}.json`),
            JSON.stringify({
              protocol: 1,
              pane: "other:p1",
              pid: owner.pid,
              start_ticks: ticks,
              socket,
            }),
          );
        const herdr = join(home, "herdr.ts");
        writeFileSync(
          herdr,
          `#!${process.execPath}\nconst r=JSON.parse(await Bun.stdin.text());let result;
if(r.method==="pane.process_info") result={process_info:{shell_pid:123,foreground_process_group_id:${mode === "foreign-terminal" ? owner.pid : 123},foreground_processes:${JSON.stringify(mode === "foreign-terminal" ? [{ pid: owner.pid, name: "unrelated" }] : [])}}};
else if(r.method==="agent.list") result={agents:[{pane_id:"owned:p1",agent_session:{source:"herdr:codex",agent:"codex",kind:"id",value:${JSON.stringify(thread)}}}]};
else throw new Error("unexpected mutation");console.log(JSON.stringify({id:r.id,result}));\n`,
        );
        chmodSync(herdr, 0o700);
        const saved = {
          ...row("a".repeat(32), "codex"),
          pane_id: pane,
          reference:
            mode === "unbound"
              ? null
              : { source: "herdr:codex", agent: "codex", kind: "id", value: thread },
        };
        const peers = reused
          ? [
              {
                ...row("b".repeat(32), "codex"),
                archived: true,
                pane_id: mode === "detached-alias" ? "other:p1" : pane,
                reference: { source: "herdr:codex", agent: "codex", kind: "id", value: peerThread },
              },
            ]
          : [];
        expect(
          (
            await command(
              home,
              ["catalog-write"],
              JSON.stringify({ revision: 0, sessions: [saved, ...peers] }),
            )
          ).code,
        ).toBe(0);
        const deleted = await command(home, ["session-delete", "--id", saved.id, "--herdr", herdr]);
        expect(deleted.code).toBe(rejected ? 1 : 0);
        expect(live(owner.pid, ticks)).toBe(retained);
        expect(live(toolPid, toolTicks)).toBe(retained);
        const catalog = object(parse((await command(home, ["catalog-read"])).stdout.toString()));
        expect(array(catalog.sessions)).toEqual(rejected ? [saved, ...peers] : peers);
        if (detached) {
          const stale = await command(
            home,
            ["catalog-write"],
            JSON.stringify({
              revision: catalog.revision,
              sessions: [saved, ...peers],
            }),
          );
          expect(object(parse(stale.stdout.toString())).sessions).toEqual(peers);
        }
        if (mode === "unbound") {
          const rediscovery = {
            ...saved,
            id: "b".repeat(64),
            reference: { source: "herdr:codex", agent: "codex", kind: "id", value: thread },
          };
          const write = await command(
            home,
            ["catalog-write"],
            JSON.stringify({ revision: catalog.revision, sessions: [rediscovery] }),
          );
          expect(object(parse(write.stdout.toString())).sessions).toEqual([]);
        }
      } finally {
        if (toolPid && toolTicks && live(toolPid, toolTicks)) process.kill(toolPid, "SIGKILL");
        owner.kill("SIGKILL");
        await owner.exited;
      }
    }, 15000);
});
for (const provider of ["dsh", "ply"] as const)
  describe(`${provider} native protocol owner`, () => {
    test("owns one session, carries images, queues turns, restores approvals and cancels native work", async () => {
      const home = fresh(),
        health = Bun.serve({
          port: 0,
          hostname: "127.0.0.1",
          fetch: () => Response.json({ data: [{ id: "fixture-model" }] }),
        });
      const upper = provider.toUpperCase(),
        extra: NodeJS.ProcessEnv = {
          DEEPSEEK_API_KEY: "owned-fixture",
          [`KITE_ENABLE_${upper}`]: "1",
          [`KITE_${upper}_COMMAND`]: JSON.stringify([process.execPath, fixture, provider]),
          [`KITE_${upper}_MODEL`]: "fixture-model",
          [`KITE_${upper}_PROVIDER`]: "fixture",
          [`KITE_${upper}_HEALTH_URL`]: `http://127.0.0.1:${health.port}/models`,
        };
      let ownerPid: number | undefined;
      const rpcs: RpcProcess[] = [];
      try {
        expect(
          (
            await command(
              home,
              ["catalog-write"],
              JSON.stringify({ version: 1, revision: 0, sessions: [row(undefined, provider)] }),
            )
          ).code,
        ).toBe(0);
        const opens = await Promise.all(
          Array.from({ length: 3 }, () =>
            command(
              home,
              ["session-open", "--id", "a".repeat(32), "--herdr", "unused"],
              undefined,
              extra,
            ),
          ),
        );
        for (const r of opens) expect(r.code, r.stderr.toString()).toBe(0);
        const agent = object(object(parse(string(opens[0]?.stdout.toString()))).agent),
          thread = string(object(agent.agent_session).value),
          path = join(home, "harnesses", thread);
        ownerPid = Number(object(parse(readFileSync(join(path, "state.json"), "utf8"))).pid);
        for (const result of opens)
          expect(object(object(parse(result.stdout.toString())).agent).pane_id).toBe(agent.pane_id);
        const rpc = () => {
          const instance = new RpcProcess(
            [process.execPath, cli, "proxy", "--pane", `kite-harness-${thread}`],
            home,
            () => {},
            () => {},
            env(home, extra),
          );
          rpcs.push(instance);
          return instance;
        };
        const client = rpc();
        await client.call("initialize", {});
        const model = string(
          object(array(object(await client.call("model/list", {})).data)[0]).model,
        );
        const turn = async (text: string) =>
          client.call("turn/start", {
            threadId: thread,
            model,
            effort: "high",
            input: [{ type: "text", text }],
          });
        expect(
          object(await client.call("thread/resume", { threadId: thread })).thread,
        ).toBeDefined();
        await expect(
          client.call("thread/resume", { threadId: crypto.randomUUID() }),
        ).rejects.toBeInstanceOf(Error);
        const first = object(object(await turn("wait then approve")).turn);
        const currentTurn = object(
          await client.call("thread/turns/list", {
            threadId: thread,
            limit: 1,
            itemsView: "notLoaded",
            sortDirection: "desc",
          }),
        );
        expect(currentTurn.data).toEqual([{ id: first.id, status: "inProgress", items: [] }]);
        await expect(
          client.call("turn/interrupt", { threadId: thread, turnId: "expired-turn" }),
        ).rejects.toBeInstanceOf(Error);
        await turn("second queued input");
        await waitStatus(path, "blocked");
        client.close();
        expect(await listening(join(path, "owner.sock"))).toBe(true);
        // Reconnect with matching native identity and respond only to a live approval.
        let permission: Json | undefined;
        const reconnect = new RpcProcess(
          [process.execPath, cli, "proxy", "--pane", `kite-harness-${thread}`],
          home,
          () => {},
          (value) => {
            permission = json(value);
          },
          env(home, extra),
        );
        rpcs.push(reconnect);
        await reconnect.call("initialize", {});
        await reconnect.call("thread/resume", { threadId: thread });
        for (let i = 0; i < 100 && !permission; i++) await sleep(20);
        check(permission, "approval not restored");
        const request = object(permission);
        check(typeof request.id === "string", "permission identity missing");
        expect(object(request.params).command).toBe("df -h\nfree -h");
        expect(object(request.params).toolName).toBe("bash");
        expect(object(request.params).reason).toBeUndefined();
        reconnect.write({ id: request.id, result: { decision: "accept" } });
        await reconnect.call("turn/interrupt", { threadId: thread, turnId: first.id });
        await waitStatus(path, "cancelled");
        expect(
          object(await reconnect.call("thread/turns/list", { threadId: thread, limit: 1 })).data,
        ).toEqual([]);
        // A new normal turn succeeds after cancellation and carries native image bytes.
        const image = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0]),
          uploadId = "e".repeat(32);
        expect(
          (
            await command(
              home,
              [
                "upload",
                "--id",
                uploadId,
                "--name",
                "owned.png",
                "--size",
                String(image.length),
                "--sha256",
                sha256(image),
              ],
              image,
            )
          ).code,
        ).toBe(0);
        await reconnect.call("turn/start", {
          threadId: thread,
          model,
          effort: "high",
          input: [
            { type: "text", text: "owned image" },
            {
              type: "localImage",
              path: join(home, ".local/share/kite/uploads", uploadId, "owned.png"),
            },
          ],
        });
        await waitStatus(path, "done");
        const history = readFileSync(join(path, "history.jsonl"), "utf8");
        expect(history).toContain("pixels");
        expect(history).toContain("function_call_output");
        expect(history).not.toContain("owned-fixture");
        await expect(
          reconnect.call("turn/start", {
            threadId: thread,
            model,
            effort: "high",
            sessionSettings: { approval_mode: "invalid" },
            input: [{ type: "text", text: "invalid policy must not execute" }],
          }),
        ).rejects.toBeInstanceOf(Error);
        await reconnect.call("turn/start", {
          threadId: thread,
          model,
          effort: "high",
          input: [{ type: "text", text: "delay queued first" }],
          sessionSettings: { approval_mode: "ask" },
        });
        await reconnect.call("turn/start", {
          threadId: thread,
          model,
          effort: "high",
          input: [{ type: "text", text: "queued second completed" }],
          sessionSettings: { approval_mode: "bypass" },
        });
        await waitStatus(path, "done");
        const queuedHistory = readFileSync(join(path, "history.jsonl"), "utf8");
        expect(queuedHistory).toMatch(/native-\d+:queued second completed/);
        expect(queuedHistory).toContain("delay queued first:text:policy-ask");
        expect(queuedHistory).toContain("queued second completed:text:policy-bypass");
        expect(queuedHistory).not.toContain("invalid policy must not execute");
        const configuration = object(object(await reconnect.call("config/read", {})).config);
        expect(object(array(configuration.session_settings)[0]).currentValue).toBe("bypass");
        const previousOwner = ownerPid;
        check(previousOwner, "owner missing");
        process.kill(previousOwner, "SIGTERM");
        for (
          let i = 0;
          i < 100 && ((await listening(join(path, "owner.sock"))) || startTicks(previousOwner));
          i++
        )
          await sleep(20);
        const restored = await command(
          home,
          ["session-open", "--id", "a".repeat(32), "--herdr", "unused"],
          undefined,
          extra,
        );
        expect(restored.code, restored.stderr.toString()).toBe(0);
        expect(object(object(parse(restored.stdout.toString())).agent).pane_id).toBe(agent.pane_id);
        ownerPid = Number(object(parse(readFileSync(join(path, "state.json"), "utf8"))).pid);
        expect(ownerPid).not.toBe(previousOwner);
        const resumed = rpc();
        await resumed.call("initialize", {});
        const lease = tryLock(join(path, "owner.lock"));
        check(lease, "owner lease unavailable");
        let accepted = false;
        const submission = resumed
          .call("turn/start", {
            threadId: thread,
            model,
            effort: "high",
            input: [{ type: "text", text: "restored owned context" }],
          })
          .then((value) => {
            accepted = true;
            return value;
          });
        try {
          await sleep(30);
          expect(accepted).toBe(false);
        } finally {
          lease();
        }
        await submission;
        await waitStatus(path, "done");
        expect(readFileSync(join(path, "history.jsonl"), "utf8")).toContain(
          "restored owned context",
        );
      } finally {
        for (const rpc of rpcs) rpc.close();
        if (ownerPid)
          try {
            process.kill(ownerPid, "SIGTERM");
          } catch {
            /* already gone */
          }
        health.stop(true);
      }
    }, 15000);
  });
test("Antigravity discovery uses native authentication and one owner restores the same conversation", async () => {
  const home = fresh(),
    id = "a".repeat(32);
  const configured = {
    KITE_ENABLE_ANTIGRAVITY: "1",
    KITE_ANTIGRAVITY_COMMAND: JSON.stringify([
      process.execPath,
      resolve("packages/host/test/fixtures/antigravity.ts"),
    ]),
  };
  const signedIn = {
    ...configured,
    KITE_FIXTURE_AGY_SIGNED_IN: "1",
    KITE_FIXTURE_AGY_SLOW_LOGIN: "1",
  };
  const scan = async (extra: NodeJS.ProcessEnv) => {
    const result = await command(home, ["harness-scan"], undefined, extra);
    expect(result.code, result.stderr.toString()).toBe(0);
    return array(object(parse(result.stdout.toString())).providers).map(object);
  };
  const provider = "antigravity";
  expect((await scan(configured)).some((row) => row.id === provider)).toBe(false);
  expect((await scan(signedIn)).find((row) => row.id === provider)).toMatchObject({
    managed_chat: true,
    models: true,
    attachments: true,
    usage: true,
    release: true,
    terminal: false,
  });
  const trace = join(home, "agy-fixture-methods");
  expect(readFileSync(trace, "utf8").trim().split("\n")).toEqual([
    "initialize",
    "session/list",
    "initialize",
    "session/list",
  ]);
  const created = await command(
    home,
    ["catalog-write"],
    JSON.stringify({ revision: 0, sessions: [row(id, provider)] }),
  );
  expect(created.code).toBe(0);
  const open = () =>
    command(home, ["session-open", "--id", id, "--herdr", "unused"], undefined, signedIn);
  const reference = (value: Awaited<ReturnType<typeof open>>) => {
    expect(value.code, value.stderr.toString()).toBe(0);
    return object(object(object(parse(value.stdout.toString())).agent).agent_session);
  };
  const [first, second] = await Promise.all([open(), open()]);
  const ref = reference(first);
  expect(reference(second)).toEqual(ref);
  expect(ref).toMatchObject({ agent: provider, source: "kite:harness", kind: "id" });
  const thread = string(ref.value),
    path = join(home, "harnesses", thread);
  const currentPid = () =>
    Number(object(parse(readFileSync(join(path, "state.json"), "utf8"))).pid);
  let pid = currentPid();
  let permission: import("@kite/protocol").RpcMessage | undefined;
  const client = () =>
    new RpcProcess(
      [process.execPath, cli, "proxy", "--pane", `kite-harness-${thread}`],
      home,
      () => {},
      (message) => {
        permission = message;
      },
      env(home, signedIn),
    );
  let rpc = client();
  const prompt = (text: string, toolPermission?: string) =>
    rpc.call("turn/start", {
      threadId: thread,
      model: "gemini-flash",
      effort: "high",
      sessionSettings: {
        mode: "plan",
        ...(toolPermission ? { tool_permission: toolPermission } : {}),
      },
      input: [{ type: "text", text }],
    });
  try {
    await rpc.call("initialize", {});
    const models = array(object(await rpc.call("model/list", {})).data).map(object);
    expect(models.find((model) => model.model === "gemini-flash")).toMatchObject({
      inputModalities: ["text", "image"],
      supportedReasoningEfforts: [
        { reasoningEffort: "medium", description: "medium" },
        { reasoningEffort: "high", description: "high" },
      ],
    });
    const config = object(object(await rpc.call("config/read", {})).config);
    expect(config.session_settings).toMatchObject([
      { id: "mode", currentValue: "default" },
      { id: "tool_permission", currentValue: "turbo" },
    ]);
    await prompt("approve", "default");
    await waitStatus(path, "blocked");
    for (let i = 0; !permission && i < 100; i++) await sleep(20);
    check(permission, "permission not delivered");
    expect(object(permission.params).command).toBe("printf kite-fixture");
    const permissionId = string(permission.id);
    rpc.write({ id: permissionId, result: { decision: "decline" } });
    await waitStatus(path, "done");
    const saved = object(parse(readFileSync(join(path, "native.json"), "utf8")));
    expect(saved).toMatchObject({ hasHistory: true, restorable: true, toolPermission: "default" });
    expect(saved.sessionId).toBe(readFileSync(join(home, "agy-fixture-session"), "utf8"));
    const lifecycle = await command(
      home,
      ["lifecycle-facts", "--herdr", "unused"],
      undefined,
      signedIn,
    );
    expect(lifecycle.code).toBe(0);
    expect(array(object(parse(lifecycle.stdout.toString())).facts)).toMatchObject([
      { session_id: id, idle: true, persisted: true },
    ]);
    const history = await command(
      home,
      ["harness-history", "--thread", thread],
      undefined,
      signedIn,
    );
    expect(history.code).toBe(0);
    const headerEnd = history.stdout.indexOf(10, history.stdout.indexOf(10) + 1);
    const transcript = gunzipSync(history.stdout.subarray(headerEnd + 1)).toString();
    expect(transcript).toMatch(
      /Checking[\s\S]*"type":"function_call"[\s\S]*Denied once[\s\S]*Continued/,
    );
    expect(transcript).not.toContain("PRIVATE_FIXTURE_THOUGHT");
    const turn = object(object(await prompt("cancel")).turn);
    for (let i = 0; i < 100; i++) {
      if (readFileSync(trace, "utf8").match(/^session\/prompt$/gm)?.length === 2) break;
      await sleep(20);
    }
    await rpc.call("turn/interrupt", { threadId: thread, turnId: turn.id });
    await waitStatus(path, "cancelled");
    expect(readFileSync(trace, "utf8")).toContain("session/cancel");
    rpc.close();
    process.kill(pid, "SIGTERM");
    for (let i = 0; i < 100 && startTicks(pid); i++) await sleep(20);
    expect(reference(await open())).toEqual(ref);
    pid = currentPid();
    rpc = client();
    await rpc.call("initialize", {});
    await prompt("continue");
    await waitStatus(path, "done");
    expect(readFileSync(join(path, "history.jsonl"), "utf8")).toContain(
      "Restored true gemini-flash high plan",
    );
    const methods = readFileSync(trace, "utf8").trim().split("\n");
    expect(methods.filter((method) => method === "session/new")).toHaveLength(1);
    expect(methods.filter((method) => method === "session/resume")).toHaveLength(2);
  } finally {
    rpc.close();
    if (startTicks(pid)) process.kill(pid, "SIGTERM");
  }
}, 15000);
test("Antigravity Turbo defaults on and preserves context, queued permissions and cold-start policy", async () => {
  const home = fresh(),
    id = "b".repeat(32);
  const configured = {
    KITE_ENABLE_ANTIGRAVITY: "1",
    KITE_FIXTURE_AGY_SIGNED_IN: "1",
    KITE_ANTIGRAVITY_COMMAND: JSON.stringify([
      process.execPath,
      resolve("packages/host/test/fixtures/antigravity.ts"),
    ]),
  };
  expect(
    (
      await command(
        home,
        ["catalog-write"],
        JSON.stringify({ revision: 0, sessions: [row(id, "antigravity")] }),
      )
    ).code,
  ).toBe(0);
  const open = () =>
    command(home, ["session-open", "--id", id, "--herdr", "unused"], undefined, configured);
  const opened = await open();
  expect(opened.code, opened.stderr.toString()).toBe(0);
  const reference = object(object(object(parse(opened.stdout.toString())).agent).agent_session);
  const thread = string(reference.value),
    path = join(home, "harnesses", thread);
  const state = () => object(parse(readFileSync(join(path, "state.json"), "utf8")));
  const saved = () => object(parse(readFileSync(join(path, "native.json"), "utf8")));
  const launches = () =>
    readFileSync(join(home, "agy-fixture-launches"), "utf8")
      .trim()
      .split("\n")
      .map((line) => object(parse(line)));
  const permissions: import("@kite/protocol").RpcMessage[] = [];
  const client = () =>
    new RpcProcess(
      [process.execPath, cli, "proxy", "--pane", `kite-harness-${thread}`],
      home,
      () => {},
      (message) => permissions.push(message),
      env(home, configured),
    );
  let rpc = client(),
    pid = Number(state().pid);
  const prompt = (toolPermission?: string) =>
    rpc.call("turn/start", {
      threadId: thread,
      model: "gemini-flash",
      effort: "high",
      sessionSettings: {
        mode: "plan",
        ...(toolPermission ? { tool_permission: toolPermission } : {}),
      },
      input: [{ type: "text", text: "approve" }],
    });
  try {
    await rpc.call("initialize", {});
    await expect(prompt("invalid")).rejects.toThrow();
    expect(launches()).toHaveLength(1);
    expect(launches()[0]?.turbo).toBe(true);
    const nativeId = saved().sessionId;
    await prompt();
    await waitStatus(path, "done");
    expect(permissions).toHaveLength(0);
    expect(saved()).toMatchObject({
      sessionId: nativeId,
      toolPermission: "turbo",
      restorable: true,
    });
    expect(Number(state().pid)).toBe(pid);
    expect(launches().map((launch) => launch.turbo)).toEqual([true]);

    await prompt("default");
    await waitStatus(path, "blocked");
    for (let i = 0; !permissions.length && i < 100; i++) await sleep(20);
    expect(permissions).toHaveLength(1);
    expect(startTicks(Number(launches()[0]?.pid))).toBeUndefined();
    // A queued Turbo turn must not approve the current turn's pending request.
    await prompt("turbo");
    expect(state().status).toBe("blocked");
    expect(launches().map((launch) => launch.turbo)).toEqual([true, false]);
    rpc.write({ id: permissions[0]?.id ?? null, result: { decision: "decline" } });
    await waitStatus(path, "done");
    expect(permissions).toHaveLength(1);
    expect(saved()).toMatchObject({ sessionId: nativeId, toolPermission: "turbo" });
    const history = readFileSync(join(path, "history.jsonl"), "utf8");
    expect(history.match(/Turbo tool ran/g)).toHaveLength(2);
    expect(history).toContain("Denied once");

    rpc.close();
    process.kill(pid, "SIGTERM");
    for (let i = 0; i < 100 && startTicks(pid); i++) await sleep(20);
    const restored = await open();
    expect(restored.code, restored.stderr.toString()).toBe(0);
    expect(object(object(object(parse(restored.stdout.toString())).agent).agent_session)).toEqual(
      reference,
    );
    pid = Number(state().pid);
    rpc = client();
    await rpc.call("initialize", {});
    await prompt();
    await waitStatus(path, "done");
    expect(permissions).toHaveLength(1);
    expect(saved().sessionId).toBe(nativeId);
    expect(launches().map((launch) => launch.turbo)).toEqual([true, false, true, true]);
    const methods = readFileSync(join(home, "agy-fixture-methods"), "utf8");
    expect(methods.match(/^session\/new$/gm)).toHaveLength(1);
    expect(methods.match(/^session\/resume$/gm)).toHaveLength(3);
    // Refuse to reopen a missing native context as a fresh, privileged session.
    rmSync(join(home, "agy-fixture-session"));
    await prompt("default");
    await waitStatus(path, "failed");
    expect(saved().sessionId).toBe(nativeId);
    expect(readFileSync(join(path, "history.jsonl"), "utf8")).toContain("Denied once");
    const failedMethods = readFileSync(join(home, "agy-fixture-methods"), "utf8");
    expect(failedMethods.match(/^session\/new$/gm)).toHaveLength(1);
    expect(failedMethods.match(/^session\/prompt$/gm)).toHaveLength(4);
  } finally {
    rpc.close();
    if (startTicks(pid)) process.kill(pid, "SIGTERM");
  }
}, 15000);
test("the host reports Codex and Antigravity quotas without session or turn RPCs", async () => {
  const home = fresh();
  const bin = privateDir(join(home, "bin"));
  writeFileSync(
    join(bin, "codex"),
    `#!${process.execPath}
import { createInterface } from "node:readline";
for await (const line of createInterface({ input: process.stdin })) {
  const msg = JSON.parse(line);
  let result;
  switch (msg.method) {
    case "initialize": result = {}; break;
    case "initialized": continue;
    case "account/read":
      if (msg.params.refreshToken !== false) throw new Error("expected read-only account request");
      result = { account: { type: "chatgpt", planType: "pro", email: "private@example.invalid" } };
      break;
    case "account/rateLimits/read":
      result = { rateLimits: { limitId: "codex", primary: { usedPercent: 25 } } };
      break;
    default: throw new Error("unexpected session or model operation");
  }
  console.log(JSON.stringify({ id: msg.id, result }));
}`,
    { mode: 0o700 },
  );
  const result = await command(home, ["account-usage"], undefined, {
    PATH: bin,
    KITE_ENABLE_ANTIGRAVITY: "1",
    KITE_FIXTURE_AGY_SIGNED_IN: "1",
    KITE_FIXTURE_AGY_USAGE_MODE: "ready",
    KITE_ANTIGRAVITY_COMMAND: JSON.stringify([
      process.execPath,
      resolve("packages/host/test/fixtures/antigravity.ts"),
      "acp",
    ]),
  });
  expect(result.code, result.stderr.toString()).toBe(0);
  const output = result.stdout.toString();
  expect(output).not.toContain("private@example.invalid");
  const [codex, antigravity] = array(parse(output));
  expect(codex).toMatchObject({
    provider: "codex",
    account: { account: { type: "chatgpt", planType: "pro" } },
    limits: { rateLimits: { primary: { usedPercent: 25 } } },
  });
  expect(antigravity).toMatchObject({
    provider: "antigravity",
    account: { account: { type: "antigravity" } },
    limits: { buckets: expect.any(Array) },
  });
  expect(existsSync(join(home, "agy-fixture-session"))).toBe(false);
}, 10000);

test("Antigravity exposes read-only quota buckets and unavailable auth states", async () => {
  const runCase = async (mode: string) => {
    const home = fresh(),
      id = "c".repeat(32);
    const configured = {
      KITE_ENABLE_ANTIGRAVITY: "1",
      KITE_FIXTURE_AGY_SIGNED_IN: "1",
      KITE_FIXTURE_AGY_USAGE_MODE: mode,
      KITE_ANTIGRAVITY_COMMAND: JSON.stringify([
        process.execPath,
        resolve("packages/host/test/fixtures/antigravity.ts"),
      ]),
    };
    expect(
      (
        await command(
          home,
          ["catalog-write"],
          JSON.stringify({ revision: 0, sessions: [row(id, "antigravity")] }),
        )
      ).code,
    ).toBe(0);
    const opened = await command(
      home,
      ["session-open", "--id", id, "--herdr", "unused"],
      undefined,
      configured,
    );
    expect(opened.code, opened.stderr.toString()).toBe(0);
    const thread = string(
      object(object(object(parse(opened.stdout.toString())).agent).agent_session).value,
    );
    const path = join(home, "harnesses", thread);
    const pid = Number(object(parse(readFileSync(join(path, "state.json"), "utf8"))).pid);
    const rpc = new RpcProcess(
      [process.execPath, cli, "proxy", "--pane", `kite-harness-${thread}`],
      home,
      () => {},
      () => {},
      env(home, configured),
    );
    try {
      await rpc.call("initialize", {});
      const account = object(await rpc.call("account/read", { refreshToken: false }));
      expect(account).toMatchObject({
        account: { type: "antigravity" },
        requiresOpenaiAuth: false,
      });
      const limits = await rpc.call("account/rateLimits/read", {});
      return object(limits);
    } finally {
      rpc.close();
      if (startTicks(pid)) process.kill(pid, "SIGTERM");
    }
  };

  const ready = await runCase("ready");
  expect(ready.buckets).toEqual([
    {
      limitId: "gemini",
      limitName: "Gemini Models",
      primary: { usedPercent: 60, windowDurationMins: 300, resetsAt: 1795439714 },
      secondary: { usedPercent: 18, windowDurationMins: 10080, resetsAt: 1796026514 },
    },
    {
      limitId: "3p",
      limitName: "Claude and GPT models",
      primary: { usedPercent: 100, windowDurationMins: 300, resetsAt: 1795456532 },
      secondary: { usedPercent: 0, windowDurationMins: 10080, resetsAt: 1796043332 },
      rateLimitReachedType: "quota_exhausted",
    },
    {
      limitId: "gemini-argon-wtu",
      limitName: "Gemini 4 Argon",
      primary: { usedPercent: 0, windowDurationMins: 10080, resetsAt: 1796043332 },
    },
  ]);

  expect(await runCase("api_key")).toMatchObject({ unavailable: "unsupportedAuth" });
  expect(await runCase("signed_out")).toMatchObject({ unavailable: "noAccount" });
  await expect(runCase("fail")).rejects.toThrow();
}, 15000);
async function waitStatus(path: string, status: string): Promise<void> {
  for (let i = 0; i < 300; i++) {
    if (object(parse(readFileSync(join(path, "state.json"), "utf8"))).status === status) return;
    await sleep(20);
  }
  throw new Error(`missing status ${status}: ${readFileSync(join(path, "state.json"), "utf8")}`);
}

test("isolated network ACP resumes one native turn, journals human approval and preserves future records", async () => {
  const home = fresh(),
    cwd = privateDir(join(home, "project"));
  const catalogFile = join(privateDir(join(home, ".local/share/kite")), "sessions.json");
  const foreign = {
    ...row("f".repeat(32), "future_provider"),
    additional: { opaque: [null, 42, "keep"] },
  };
  const delegation = {
    parent: { machine: "caller-machine", sessionId: "caller-session", provider: "claude" },
    policy: "strict",
    label: "existing child",
  };
  const existing = Array.from({ length: 15 }, (_, index) => ({
    ...row(index.toString(16).padStart(32, "0")),
    cwd,
    delegation,
  }));
  writeFileSync(
    catalogFile,
    JSON.stringify({ version: 1, revision: 1, sessions: [...existing, foreign] }),
    { mode: 0o600 },
  );
  const nodeSocket = join(home, "fixture-node.sock");
  const node = createServer((socket) => {
    socket.on("error", () => socket.destroy());
    socket.resume();
  });
  await new Promise<void>((resolve, reject) => {
    node.once("error", reject);
    node.listen(nodeSocket, resolve);
  });
  chmodSync(nodeSocket, 0o600);
  const config = join(privateDir(join(home, ".config/kite")), "node.json");
  writeFileSync(config, JSON.stringify({ socket: nodeSocket }), { mode: 0o600 });
  const health = Bun.serve({
    port: 0,
    hostname: "127.0.0.1",
    fetch: () => Response.json({ data: [{ id: "fixture-model" }] }),
  });
  const trace = join(home, "native.jsonl");
  const extra = {
    DEEPSEEK_API_KEY: "owned-fixture",
    KITE_ENABLE_PLY: "1",
    KITE_PLY_MODEL: "fixture-model",
    KITE_PLY_PROVIDER: "fixture",
    KITE_PLY_HEALTH_URL: `http://127.0.0.1:${health.port}/models`,
    KITE_PLY_COMMAND: JSON.stringify([process.execPath, fixture, "ply", "--record", trace]),
  };
  const tokenFile = join(home, "test-token"),
    peers = join(home, "test-peers.json");
  const server = Bun.spawn(
    [
      process.execPath,
      cli,
      "acp",
      "serve",
      "--workspace",
      cwd,
      "--port",
      "0",
      "--provider",
      "ply",
      "--token-file",
      tokenFile,
      "--herdr",
      join(home, "no-herdr"),
    ],
    {
      env: env(home, extra),
      stdin: "ignore",
      stdout: "pipe",
      stderr: "ignore",
    },
  );
  const output = lines(server.stdout, 4096)[Symbol.asyncIterator]();
  let ownerPid: number | undefined, ownerTicks: string | undefined, native: RpcProcess | undefined;
  let legacy: AcpRpc | undefined;
  try {
    const endpoint = object(parse(string((await output.next()).value)));
    const peer = { url: string(endpoint.url), tokenFile };
    writeFileSync(peers, JSON.stringify({ peers: { isolated: peer } }), { mode: 0o600 });
    const control = (action: string, args: string[] = [], prompt?: string) =>
      command(
        home,
        ["acp", action, "--peers", peers, "--peer", "isolated", ...args],
        prompt,
        extra,
      );
    const result = async (action: string, args: string[] = [], prompt?: string) => {
      const value = await control(action, args, prompt);
      expect(value.code, value.stderr.toString()).toBe(0);
      return object(parse(value.stdout.toString()));
    };

    legacy = new AcpRpc();
    await legacy.connect(peer);
    const hello = object(await legacy.call("initialize", { protocolVersion: 1 }));
    expect(object(object(hello._meta).kite).controlPlaneVersion).toBe(2);
    expect(array(object(await legacy.call("session/list", {})).sessions)).toHaveLength(15);
    legacy.close();
    const model = JSON.stringify(["fixture", "fixture-model"]);
    const create = [
      "--cwd",
      cwd,
      "--provider",
      "ply",
      "--parent-machine",
      "caller-machine",
      "--parent-session",
      "caller-session",
      "--label",
      "approval fixture",
      "--model",
      model,
      "--effort",
      "off",
      "--policy",
      "human_in_the_loop",
      "--write-lock",
    ];
    const spawned = await result("spawn", create),
      id = string(spawned.sessionId);
    const thread = string(object(object(spawned._meta).kite).threadId);
    const statePath = join(home, "harnesses", thread, "state.json");
    ({ pid: ownerPid, start_ticks: ownerTicks } = JSON.parse(readFileSync(statePath, "utf8")));
    const overflow = await control("spawn", create);
    expect(overflow.code).toBe(1);
    expect(overflow.stderr.toString()).toContain("quota");
    expect(
      array((await result("list", ["--parent-session", "caller-session"])).sessions),
    ).toHaveLength(16);
    const catalog = object(parse((await command(home, ["catalog-read"])).stdout.toString()));
    const rewrite = array(catalog.sessions).map((value) => {
      const { delegation: _delegation, acp_selection: _selection, ...copy } = object(value);
      return copy;
    });
    const saved = await command(
      home,
      ["catalog-write"],
      JSON.stringify({ revision: catalog.revision, sessions: rewrite }),
    );
    expect(saved.code).toBe(0);
    const diskRows = array(object(parse(readFileSync(catalogFile, "utf8"))).sessions).map(object);
    expect(diskRows.find((r) => r.id === foreign.id)).toEqual(foreign);
    expect(diskRows.find((r) => r.id === id)?.delegation).toMatchObject({
      label: "approval fixture",
    });

    const session = ["--session", id];
    const invalid = await control("send", [...session, "--effort", "nonexistent"], "must not run");
    expect(invalid.code).toBe(1);
    const sent = await result("send", session, "approve owned fixture");
    expect(sent.state).toBe("working");
    expect(typeof sent.turnId).toBe("string");
    // send has exited and closed its WebSocket. A different connection observes the same turn.
    const wait = await result("wait", [...session, "--timeout-ms", "5000"]);
    expect(wait.timedOut).toBe(false);
    expect(array(wait.sessions)[0]).toMatchObject({
      state: "waiting_approval",
      activeTurnId: sent.turnId,
      model,
      effort: "off",
      pendingInteraction: { command: "df -h\nfree -h", cwd },
    });
    expect((await result("release", session)).released).toBe(false);
    const facts = object(
      parse(
        (
          await command(
            home,
            ["notification-facts", "--herdr", join(home, "no-herdr")],
            undefined,
            extra,
          )
        ).stdout.toString(),
      ),
    );
    expect(array(facts.facts)).toContainEqual(
      expect.objectContaining({
        session_id: id,
        status: "blocked",
        interaction: expect.objectContaining({ kind: "approval", command: "df -h\nfree -h", cwd }),
      }),
    );
    const conflict = await control("spawn", [
      "--cwd",
      cwd,
      "--provider",
      "ply",
      "--write-lock",
      "--standalone",
    ]);
    expect(conflict.code).toBe(1);
    expect(conflict.stderr.toString()).toContain("workspace");

    const approval = Promise.withResolvers<string>();
    native = new RpcProcess(
      [process.execPath, cli, "proxy", "--pane", `kite-harness-${thread}`],
      cwd,
      () => {},
      (value) => approval.resolve(string(value.id)),
      env(home, extra),
    );
    await native.call("initialize", {});
    await native.call("thread/resume", { threadId: thread });
    native.write({ id: await approval.promise, result: { decision: "accept" } });
    await waitStatus(join(home, "harnesses", thread), "done");
    const completed = await result("wait", [...session, "--timeout-ms", "5000"]);
    expect(array(completed.sessions)[0]).toMatchObject({
      state: "idle",
      lastTurnId: sent.turnId,
      lastTurnStatus: "completed",
    });
    const read = await result("read", [...session, "--turn", string(sent.turnId)]);
    expect(read.reply).toContain("approve owned fixture");
    expect(array(read.tools)).toContainEqual(
      expect.objectContaining({ command: "df -h\nfree -h", output: "owned result", exitCode: 0 }),
    );
    const nativeStarts = readFileSync(trace, "utf8")
      .trim()
      .split("\n")
      .map((line) => object(parse(line)))
      .filter((event) => event.event === "spawn");
    expect(nativeStarts).toHaveLength(1);
    expect(nativeStarts[0]?.kiteSessionId).toBe(id);

    // Removing the owned gateway projection exercises recovery from the native turn markers.
    rmSync(join(home, ".local/share/kite/acp", `${id}-turns.json`));
    const recovered = await result("read", [...session, "--turn", string(sent.turnId)]);
    expect(recovered.reply).toBe(read.reply);
    expect(array(recovered.tools)[0]).toMatchObject({ exitCode: 0, status: "completed" });
    expect((await result("cancel", session)).state).toBe("idle");
  } finally {
    legacy?.close();
    native?.close();
    server.kill();
    await server.exited;
    await output.return?.(undefined);
    if (ownerPid && ownerTicks && live(ownerPid, ownerTicks)) {
      process.kill(ownerPid, "SIGTERM");
      for (let attempt = 0; attempt < 50 && live(ownerPid, ownerTicks); attempt++) await sleep(20);
    }
    node.close();
    health.stop(true);
  }
}, 20_000);
