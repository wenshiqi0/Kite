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
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { array, check, type Json, json, object, parse, string } from "@kite/protocol";
import { bounded, live, privateDir, sha256, sleep, startTicks, tryLock } from "@kite/runtime";
import { RpcProcess } from "@kite/runtime/rpc";
import { listening } from "@kite/runtime/socket";

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
  KITE_AGENT_RESOURCE_CONTROL: "0",
  KITE_ENABLE_DSH: "0",
  KITE_ENABLE_PLY: "0",
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

describe("explicit owner termination", () => {
  for (const mode of ["bound", "unbound", "shared", "foreign-terminal"])
    test(mode === "foreign-terminal"
      ? "an unrelated terminal foreground is never signalled"
      : mode === "shared"
        ? "a shared loaded backend and its tools are retained"
        : `deletion stops a real ${mode} owner and its stubborn tool before removing the record`, async () => {
      const shared = mode === "shared",
        retained = shared || mode === "foreign-terminal";
      const home = fresh(),
        path = privateDir(join(home, "owner")),
        socket = join(path, "backend.sock"),
        thread = crypto.randomUUID(),
        pane = "owned:p1";
      const owner = Bun.spawn(
        [
          process.execPath,
          resolve("packages/host/test/fixtures/deletion-owner.ts"),
          socket,
          JSON.stringify(shared ? [thread, crypto.randomUUID()] : [thread]),
          path,
          "app-server",
          `unix://${socket}`,
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
        expect(
          (
            await command(
              home,
              ["catalog-write"],
              JSON.stringify({ revision: 0, sessions: [saved] }),
            )
          ).code,
        ).toBe(0);
        const deleted = await command(home, ["session-delete", "--id", saved.id, "--herdr", herdr]);
        expect(deleted.code).toBe(retained ? 1 : 0);
        expect(live(owner.pid, ticks)).toBe(retained);
        expect(live(toolPid, toolTicks)).toBe(retained);
        const catalog = object(parse((await command(home, ["catalog-read"])).stdout.toString()));
        expect(array(catalog.sessions)).toHaveLength(retained ? 1 : 0);
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
async function waitStatus(path: string, status: string): Promise<void> {
  for (let i = 0; i < 300; i++) {
    if (object(parse(readFileSync(join(path, "state.json"), "utf8"))).status === status) return;
    await sleep(20);
  }
  throw new Error(`missing status ${status}: ${readFileSync(join(path, "state.json"), "utf8")}`);
}
