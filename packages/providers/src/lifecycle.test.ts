import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { json, object, type SessionRecord } from "@kite/protocol";
import { privateDir, same, startTicks } from "@kite/runtime";
import { catalogRead, catalogWrite } from "./catalog";
import { harnessDirectory } from "./harness";
import {
  type HarnessLifecycle,
  type LifecycleFact,
  lifecycleReap,
  managedLifecycleFact,
} from "./lifecycle";

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
