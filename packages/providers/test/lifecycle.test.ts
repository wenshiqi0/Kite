import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { json, object, type SessionRecord } from "@kite/protocol";
import { privateDir, same, startTicks } from "@kite/runtime";
import { catalogRead, catalogWrite } from "../src/catalog";
import { registryPath, sessionOpen } from "../src/codex";
import { harnessDirectory } from "../src/harness";
import {
  archiveReleasedCodex,
  type HarnessLifecycle,
  type LifecycleFact,
  lifecycleReap,
  managedLifecycleFact,
} from "../src/lifecycle";

let root = "",
  previousHome: string | undefined,
  previousState: string | undefined;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "kite-lifecycle-test-"));
  previousHome = process.env.HOME;
  previousState = process.env.KITE_HARNESS_STATE_DIR;
  process.env.HOME = root;
  process.env.KITE_HARNESS_STATE_DIR = join(root, "harnesses");
});
afterEach(() => {
  if (previousHome === undefined) delete process.env.HOME;
  else process.env.HOME = previousHome;
  if (previousState === undefined) delete process.env.KITE_HARNESS_STATE_DIR;
  else process.env.KITE_HARNESS_STATE_DIR = previousState;
  rmSync(root, { recursive: true, force: true });
});
function record(): SessionRecord {
  const thread = crypto.randomUUID();
  return {
    id: "a".repeat(32),
    provider: "dsh",
    title: "Owned archive",
    pane_id: `kite-harness-${thread}`,
    archived: true,
    reference: { source: "kite:harness", agent: "dsh", kind: "id", value: thread },
  };
}
function owned(r: SessionRecord): LifecycleFact {
  return {
    session_id: r.id,
    pane: r.pane_id || "",
    owner: { pane: r.pane_id || "", pid: process.pid, start_ticks: startTicks(process.pid) || "0" },
    reference: r.reference,
    archived: true,
    idle: true,
    persisted: true,
    activity: "owned-activity",
    last_activity_ms: 1,
    tui_pid: null,
  };
}
function adapter(fact: LifecycleFact, release: () => void = () => {}): HarnessLifecycle {
  return {
    snapshot: async () => fact,
    ownerLock: () => join(root, "owner.lock"),
    sharesOwner: (r, _owner, other) => same(r.reference, other.reference),
    release: async (r) => {
      release();
      return { reclaimed: true, session_id: r.id };
    },
  };
}
test("only archived requests can release a durable idle managed owner", async () => {
  const r = record(),
    fact = owned(r);
  await catalogWrite({ version: 1, revision: 0, sessions: [r] });
  let released = 0;
  const a = adapter(fact, () => released++);
  expect(
    await lifecycleReap(object(json({ ...fact, reason: "archived" })), "unused", { dsh: a }),
  ).toEqual({ reclaimed: true, session_id: r.id });
  expect(released).toBe(1);
  await expect(
    lifecycleReap(object(json({ ...fact, reason: "idle_timeout" })), "unused", { dsh: a }),
  ).rejects.toThrow("invalid lifecycle");
  expect(released).toBe(1);
});
test("working, unsaved, changed or shared owners are retained", async () => {
  const r = record(),
    fact = owned(r);
  await catalogWrite({ version: 1, revision: 0, sessions: [r] });
  for (const variant of [
    { ...fact, idle: false },
    { ...fact, persisted: false },
    { ...fact, activity: "changed" },
  ]) {
    const a = adapter(variant, () => {
      throw new Error("unsafe release");
    });
    expect(
      object(
        await lifecycleReap(object(json({ ...fact, reason: "archived" })), "unused", { dsh: a }),
      ).reclaimed,
    ).toBe(false);
  }
  const catalog = await catalogRead();
  await catalogWrite({ ...catalog, sessions: [r, { ...r, id: "b".repeat(32) }] });
  expect(
    await lifecycleReap(object(json({ ...fact, reason: "archived" })), "unused", {
      dsh: adapter(fact, () => {
        throw new Error("shared owner released");
      }),
    }),
  ).toEqual({ reclaimed: false, reason: "shared_owner" });
});
test("catalog changes during the release decision are revalidated under its final lease", async () => {
  const r = record(),
    fact = owned(r);
  await catalogWrite({ version: 1, revision: 0, sessions: [r] });
  const a = adapter(fact, () => {
    throw new Error("unarchived owner released");
  });
  a.snapshot = async () => {
    const catalog = await catalogRead();
    await catalogWrite({ ...catalog, sessions: [{ ...r, archived: false }] });
    return fact;
  };
  expect(
    await lifecycleReap(object(json({ ...fact, reason: "archived" })), "unused", { dsh: a }),
  ).toEqual({ reclaimed: false, reason: "record_changed" });
});
test("Codex archives sync only for released sessions and restore the original thread", async () => {
  const path = process.env.PATH;
  const codexHome = process.env.CODEX_HOME;
  process.env.PATH = `${root}:${path || ""}`;
  process.env.CODEX_HOME = join(root, "codex-home");
  try {
    const active = privateDir(join(process.env.CODEX_HOME, "sessions"));
    const archived = privateDir(join(process.env.CODEX_HOME, "archived_sessions"));
    const makeRecord = (name: string, pane_id: string) => {
      const thread = crypto.randomUUID();
      const file = `rollout-2026-11-04T12-00-00-${thread}.jsonl`;
      writeFileSync(join(active, file), name);
      return {
        file,
        record: {
          id: name.repeat(32),
          provider: "codex",
          title: name,
          pane_id,
          archived: true,
          reference: { source: "herdr:codex", agent: "codex", kind: "id", value: thread },
        } satisfies SessionRecord,
      };
    };
    const finished = makeRecord("a", "w1:p1");
    const running = makeRecord("b", "w2:p1");
    const owned = makeRecord("c", "w3:p1");
    const shared = makeRecord("d", "w4:p1");
    const alias = { ...shared.record, id: "e".repeat(32), archived: false };
    writeFileSync(registryPath("w3:p1"), "{}");
    writeFileSync(
      join(root, "codex"),
      `#!/bin/sh
set -eu
case "$1" in
  archive) from=sessions; to=archived_sessions ;;
  unarchive) from=archived_sessions; to=sessions ;;
  *) exit 2 ;;
esac
mv "$CODEX_HOME/$from/"*"-$2.jsonl" "$CODEX_HOME/$to/"
`,
      { mode: 0o700 },
    );
    const herdr = join(root, "herdr");
    const writeAgent = (record: SessionRecord) => {
      const agent = { agent: "codex", pane_id: record.pane_id, agent_session: record.reference };
      writeFileSync(
        herdr,
        `#!/bin/sh\nread -r request\nprintf '%s\\n' '${JSON.stringify({ result: { agents: [agent] } })}'\n`,
        { mode: 0o700 },
      );
      return agent;
    };
    writeAgent(running.record);
    await catalogWrite({
      version: 1,
      revision: 0,
      sessions: [finished.record, running.record, owned.record, shared.record, alias],
    });

    expect(await archiveReleasedCodex(herdr)).toEqual({ archived: 1, failed: 0 });
    expect(await archiveReleasedCodex(herdr)).toEqual({ archived: 0, failed: 0 });
    expect(existsSync(join(archived, finished.file))).toBe(true);
    for (const { file } of [running, owned, shared])
      expect(existsSync(join(active, file))).toBe(true);

    const catalog = await catalogRead();
    await catalogWrite({
      ...catalog,
      sessions: catalog.sessions.map((r) =>
        r.id === finished.record.id ? { ...r, archived: false } : r,
      ),
    });
    expect(existsSync(join(active, finished.file))).toBe(true);

    const restored = await catalogRead();
    await catalogWrite({
      ...restored,
      sessions: restored.sessions.map((r) =>
        r.id === finished.record.id ? { ...r, archived: true } : r,
      ),
    });
    expect(await archiveReleasedCodex(herdr)).toEqual({ archived: 1, failed: 0 });
    const agent = writeAgent(finished.record);
    expect(await sessionOpen(finished.record.id, herdr)).toEqual(json({ agent }));
    expect(existsSync(join(active, finished.file))).toBe(true);
    expect((await catalogRead()).sessions.find((r) => r.id === finished.record.id)).toMatchObject({
      archived: false,
      reference: finished.record.reference,
    });
  } finally {
    if (path === undefined) delete process.env.PATH;
    else process.env.PATH = path;
    if (codexHome === undefined) delete process.env.CODEX_HOME;
    else process.env.CODEX_HOME = codexHome;
  }
});
test("managed snapshots require the matching process identity and saved native context", async () => {
  const r = record(),
    thread = r.reference?.value || "",
    path = privateDir(harnessDirectory(thread));
  writeFileSync(
    join(path, "session.json"),
    JSON.stringify({
      id: r.id,
      thread,
      provider: "dsh",
      route: "fixture",
      cwd: root,
      title: r.title,
      command: ["fixture"],
      model: "fixture",
    }),
  );
  const state = {
    protocol: 2,
    status: "done",
    revision: 1,
    pid: process.pid,
    start_ticks: startTicks(process.pid),
    turn_id: "owned-turn",
  };
  writeFileSync(join(path, "state.json"), JSON.stringify(state));
  writeFileSync(join(path, "history.jsonl"), "owned history\n");
  expect((await managedLifecycleFact(r)).persisted).toBe(false);
  writeFileSync(
    join(path, "native.json"),
    JSON.stringify({ context: "x".repeat(1024 * 1024 + 16) }),
  );
  writeFileSync(join(path, "queued.json"), "[]");
  const snap = await managedLifecycleFact(r);
  expect(snap.idle && snap.persisted).toBe(true);
  writeFileSync(join(path, "queued.json"), '["pending"]');
  expect((await managedLifecycleFact(r)).idle).toBe(false);
  writeFileSync(join(path, "state.json"), JSON.stringify({ ...state, start_ticks: "invalid" }));
  await expect(managedLifecycleFact(r)).rejects.toThrow("identity changed");
});
