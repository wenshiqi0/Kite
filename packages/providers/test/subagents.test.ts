import { afterAll, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type ObjectValue, object } from "@kite/protocol";
import { attachSubagents, codexSubagents, subagentFacts } from "../src/subagents";

const root = mkdtempSync(join(tmpdir(), "kite-subagents-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const parent = "01a1a48d-7305-7331-be8f-c5b6974b635b";
const child = "01a1a6d8-cd2a-7f21-8c70-bac965b3e1df";
const grandchild = "01a1a6d9-cd2a-7f21-8c70-bac965b3e1df";
const sibling = "01a1a6da-cd2a-7f21-8c70-bac965b3e1df";
const foreign = "01a1a6db-cd2a-7f21-8c70-bac965b3e1df";

function thread(id: string, owner: string, status = "active"): ObjectValue {
  return {
    id,
    parentThreadId: owner,
    agentNickname: id === child ? "Franklin" : "Helper",
    source: {
      subAgent: {
        thread_spawn: {
          parent_thread_id: owner,
          agent_path: `/root/${id === child ? "research" : "verify"}`,
          depth: 1,
        },
      },
    },
    status: { type: status, activeFlags: [] },
    updatedAt: 1793800000,
  };
}

test("native discovery includes archived descendants and produces a verified parent-first tree", async () => {
  const calls: ObjectValue[] = [];
  const result = await codexSubagents(parent, async (method, params) => {
    expect(method).toBe("thread/list");
    const p = object(params);
    calls.push(p);
    expect(p.ancestorThreadId).toBe(parent);
    expect(p.sourceKinds).toEqual(["subAgentThreadSpawn"]);
    expect(p.useStateDbOnly).toBe(true);
    if (p.archived) return { data: [thread(sibling, parent, "notLoaded")], nextCursor: null };
    return {
      data: [
        thread(grandchild, child),
        thread(foreign, "01a1a6dc-cd2a-7f21-8c70-bac965b3e1df"),
        thread(child, parent),
      ],
      nextCursor: null,
    };
  });
  expect(calls).toHaveLength(2);
  expect(result.map((x) => [x.id, x.parent_id])).toEqual([
    [child, parent],
    [grandchild, child],
    [sibling, parent],
  ]);
  expect(result[0]).toMatchObject({
    name: "Franklin",
    task: "research",
    runtime_status: "active",
    updated_ms: 1793800000000,
  });
  expect(result[2]?.runtime_status).toBe("notLoaded");
  expect(result[2]?.last_outcome).toBeNull();
  expect(JSON.stringify(result)).not.toContain("source");
});

test("pagination keeps descendants whose parent arrives later and rejects repeated cursors", async () => {
  const result = await codexSubagents(parent, async (_, params) => {
    const p = object(params);
    if (p.archived) return { data: [], nextCursor: null };
    return p.cursor
      ? { data: [thread(child, parent)], nextCursor: null }
      : { data: [thread(grandchild, child)], nextCursor: "page2" };
  });
  expect(result.map((x) => x.id)).toEqual([child, grandchild]);
  await expect(
    codexSubagents(parent, async () => ({ data: [], nextCursor: "same" })),
  ).rejects.toThrow("repeated");
});

test("native runtime flags remain facts for the Rust activity rules", () => {
  for (const flag of ["waitingOnApproval", "waitingOnUserInput"]) {
    expect(subagentFacts({ status: { type: "active", activeFlags: [flag] } })).toEqual({
      runtime_status: "active",
      active_flags: [flag],
      last_outcome: null,
    });
  }
});

test("bounded persisted outcomes survive unload, and a follow-up is not mistaken for completion", () => {
  const previous = process.env.CODEX_HOME;
  process.env.CODEX_HOME = root;
  mkdirSync(join(root, "sessions"), { recursive: true });
  const path = join(root, "sessions", "rollout.jsonl");
  const event = (type: string, error?: string) =>
    `${JSON.stringify({ type: "event_msg", payload: { type, error } })}\n`;
  try {
    for (const [tail, expected] of [
      [event("task_complete"), "completed"],
      [event("task_complete", "capacity exceeded"), "failed"],
      [event("turn_aborted"), "aborted"],
      [event("task_complete") + event("task_started"), null],
    ] satisfies [string, string | null][]) {
      writeFileSync(path, `${"x".repeat(150000)}\n${tail}{"partial":`);
      expect(subagentFacts({ status: { type: "notLoaded" }, path }).last_outcome).toBe(expected);
      expect(
        subagentFacts({ status: { type: "active", activeFlags: [] }, path }).last_outcome,
      ).toBeNull();
    }
    const outside = join(root, "outside.jsonl");
    writeFileSync(outside, event("task_complete"));
    expect(subagentFacts({ status: { type: "notLoaded" }, path: outside }).last_outcome).toBeNull();
    const link = join(root, "sessions", "link.jsonl");
    symlinkSync(path, link);
    expect(subagentFacts({ status: { type: "notLoaded" }, path: link }).last_outcome).toBeNull();
  } finally {
    if (previous === undefined) delete process.env.CODEX_HOME;
    else process.env.CODEX_HOME = previous;
  }
});

test("discovery failures do not detach or retarget the parent, and skip other providers", async () => {
  const agent = {
    agent: "codex",
    pane_id: "owned:p1",
    agent_session: { source: "herdr:codex", agent: "codex", kind: "id", value: parent },
  };
  const claude = { ...agent, agent: "claude" };
  const called: string[] = [];
  const result = await attachSubagents([agent, claude], async (pane, id) => {
    called.push(pane);
    expect(id).toBe(parent);
    throw new Error("offline");
  });
  expect(called).toEqual(["owned:p1"]);
  expect(result[0]).toEqual({ ...agent, subagents: null, subagents_error: true });
  expect(result[1]).toEqual(claude);
  expect(await attachSubagents([agent], async () => undefined)).toEqual([agent]);
});
