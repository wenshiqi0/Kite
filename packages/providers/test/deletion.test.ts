import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { SessionRecord } from "@kite/protocol";
import { tryLock } from "@kite/runtime";
import { catalogRead, catalogRoot, catalogWrite, patchSession } from "../src/catalog";
import { type SessionTerminator, sessionDelete } from "../src/deletion";

let root = "",
  previousHome: string | undefined;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "kite-delete-test-"));
  previousHome = process.env.HOME;
  process.env.HOME = root;
});
afterEach(() => {
  if (previousHome === undefined) delete process.env.HOME;
  else process.env.HOME = previousHome;
  rmSync(root, { recursive: true, force: true });
});
function record(archived = false): SessionRecord {
  return {
    id: "a".repeat(32),
    provider: "codex",
    title: "Disposable session",
    archived,
    pane_id: "owned:p1",
    terminal_id: "owned-terminal",
    reference: { source: "herdr:codex", agent: "codex", kind: "id", value: crypto.randomUUID() },
  };
}
function adapter(terminate: (record: SessionRecord) => Promise<void>): SessionTerminator {
  return {
    ownerLock: () => join(root, "owner.lock"),
    sharesOwner: () => false,
    terminate: async (record) => {
      await terminate(record);
      return undefined;
    },
  };
}
for (const archived of [false, true])
  test(`deletes ${archived ? "archived" : "active"} work only after its owner and tools stop`, async () => {
    const r = record(archived),
      other = { ...record(), id: "b".repeat(32), pane_id: "other:p1" };
    await catalogWrite({ revision: 0, sessions: [r, other] });
    const history = join(root, "provider-history.jsonl");
    writeFileSync(history, "provider-owned history");
    const child = Bun.spawn([process.execPath, "-e", "setInterval(()=>{},1000)"], {
      stdout: "ignore",
      stderr: "ignore",
    });
    try {
      const result = await sessionDelete(r.id, "unused", {
        codex: adapter(async () => {
          expect(existsSync(join(catalogRoot(), "deleted-sessions"))).toBe(false);
          expect(
            JSON.parse(await Bun.file(join(catalogRoot(), "sessions.json")).text()).sessions,
          ).toHaveLength(2);
          child.kill();
          await child.exited;
        }),
      });
      expect(child.signalCode).toBe("SIGTERM");
      expect(result.sessions).toEqual([other]);
      expect(result.revision).toBe(2);
      expect((await sessionDelete(r.id, "unused")).sessions).toEqual([other]);
      expect((await catalogRead()).sessions).toEqual([other]);
      expect(await Bun.file(history).text()).toBe("provider-owned history");
    } finally {
      child.kill();
      await child.exited;
    }
  });
test("failed release keeps the record visible and permits a later retry", async () => {
  const r = record();
  await catalogWrite({ revision: 0, sessions: [r] });
  await expect(
    sessionDelete(r.id, "unused", {
      codex: adapter(async () => {
        throw new Error("scope still populated");
      }),
    }),
  ).rejects.toThrow("scope still populated");
  expect((await catalogRead()).sessions).toEqual([r]);
  expect(existsSync(join(catalogRoot(), "deleted-sessions"))).toBe(false);
  expect(
    (await sessionDelete(r.id, "unused", { codex: adapter(async () => {}) })).sessions,
  ).toEqual([]);
});
test("drafts and already stopped sessions delete without a running provider", async () => {
  const r = { ...record(), reference: null, pane_id: "", terminal_id: "" };
  await catalogWrite({ revision: 0, sessions: [r] });
  expect((await sessionDelete(r.id, "unavailable", {})).sessions).toEqual([]);
  await expect(sessionDelete("../unsafe", "unused", {})).rejects.toThrow("invalid session ID");
});
test("shared references, panes and alias owners never terminate another session", async () => {
  for (const kind of ["reference", "pane", "owner"]) {
    const r = { ...record(), id: crypto.randomUUID().replaceAll("-", "") };
    const other = {
      ...record(),
      id: crypto.randomUUID().replaceAll("-", ""),
      pane_id: "other:p1",
      ...(kind === "reference" ? { reference: r.reference } : {}),
      ...(kind === "pane" ? { pane_id: r.pane_id } : {}),
    };
    const catalog = await catalogRead();
    await catalogWrite({ ...catalog, sessions: [r, other] });
    const a = adapter(async () => {
      throw new Error("another session was stopped");
    });
    a.sharesOwner = () => kind === "owner";
    await expect(sessionDelete(r.id, "unused", { codex: a })).rejects.toThrow(
      "session owner is shared",
    );
    expect((await catalogRead()).sessions).toHaveLength(2);
  }
});
test("the owner lease fences resume and the catalog lease fences stale writes", async () => {
  const r = record();
  await catalogWrite({ revision: 0, sessions: [r] });
  let started!: () => void, finish!: () => void;
  const entered = new Promise<void>((resolve) => {
    started = resolve;
  });
  const gate = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const deleting = sessionDelete(r.id, "unused", {
    codex: adapter(async () => {
      started();
      await gate;
    }),
  });
  await entered;
  expect(tryLock(join(root, "owner.lock"))).toBeUndefined();
  expect(tryLock(join(catalogRoot(), "sessions.lock"))).toBeUndefined();
  const staleWrite = catalogWrite({ revision: 1, sessions: [r] });
  finish();
  await deleting;
  expect(await staleWrite).toEqual({ conflict: true });
  const fresh = await catalogRead();
  const rediscovery = { ...r, id: "c".repeat(64) };
  expect(await catalogWrite({ ...fresh, sessions: [r, rediscovery] })).toMatchObject({
    sessions: [],
  });
  expect((await catalogRead()).sessions).toEqual([]);
});
test("session identity is rechecked after waiting for its owner lease", async () => {
  const r = record();
  await catalogWrite({ revision: 0, sessions: [r] });
  const a = adapter(async () => {
    throw new Error("changed session stopped");
  });
  a.ownerLock = () => {
    void patchSession(r.id, {
      reference: { source: "herdr:codex", agent: "codex", kind: "id", value: crypto.randomUUID() },
    });
    return join(root, "owner.lock");
  };
  await expect(sessionDelete(r.id, "unused", { codex: a })).rejects.toThrow(
    "session changed during deletion",
  );
  expect((await catalogRead()).sessions).toHaveLength(1);
});
