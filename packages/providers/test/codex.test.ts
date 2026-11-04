import { afterAll, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ObjectValue } from "@kite/protocol";
import { herdrRequest, reconcileAgentSessions, reportSessionBinding } from "../src/codex";

const root = mkdtempSync(join(tmpdir(), "kite-codex-binding-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));

function herdrFixture(ignore: boolean): { binary: string; state: string } {
  const name = crypto.randomUUID(),
    binary = join(root, `${name}.ts`),
    state = join(root, `${name}.json`);
  writeFileSync(
    state,
    JSON.stringify({ thread: "validation-thread", seq: (Date.now() - 1000) * 1_000_000 }),
  );
  writeFileSync(
    binary,
    `#!${process.execPath}
import { readFileSync, writeFileSync } from "node:fs";
const state = ${JSON.stringify(state)};
const request = JSON.parse(await Bun.stdin.text());
const current = JSON.parse(readFileSync(state, "utf8"));
let result;
if (request.method === "pane.report_agent_session") {
  const p = request.params;
  if (!p.resume_argv?.length || p.resume_argv[0].includes("/")) {
    console.log(JSON.stringify({ id: request.id, error: { code: "invalid_resume_argv", message: "resume_argv must start with a plain command name" } }));
    process.exit(0);
  }
  // Match Herdr's sequence and same-owner session replacement guards.
  if (!${ignore} && p.seq > current.seq && p.session_start_source === "resume") {
    writeFileSync(state, JSON.stringify({ thread: p.agent_session_id, seq: p.seq, resume: p.resume_argv }));
  }
  result = { type: "ok" };
} else if (request.method === "agent.list") {
  result = { agents: [{ pane_id: "owned:p1", agent_session: {
    source: "herdr:codex", agent: "codex", kind: "id", value: current.thread,
  } }] };
} else {
  throw new Error("unexpected mutation: " + request.method);
}
console.log(JSON.stringify({ id: request.id, result }));
`,
  );
  chmodSync(binary, 0o700);
  return { binary, state };
}

test("reconciles a stale Herdr thread after a sequenced native SessionStart report", async () => {
  const { binary, state } = herdrFixture(false);
  await reportSessionBinding("owned:p1", binary, "original-thread");
  expect(JSON.parse(readFileSync(state, "utf8")).thread).toBe("original-thread");
  expect(JSON.parse(readFileSync(state, "utf8")).resume.slice(0, 3)).toEqual([
    "kite-node",
    "backend",
    "tui",
  ]);
});

test("does not treat an acknowledged but unapplied session report as a successful binding", async () => {
  const { binary, state } = herdrFixture(true);
  await expect(reportSessionBinding("owned:p1", binary, "original-thread")).rejects.toThrow(
    "did not apply",
  );
  expect(JSON.parse(readFileSync(state, "utf8")).thread).toBe("validation-thread");
});

function agent(thread: string, pane = "parent:p1"): ObjectValue {
  return {
    terminal_id: "parent-terminal",
    pane_id: pane,
    agent: "codex",
    agent_status: "working",
    interactive_ready: true,
    agent_session: { source: "herdr:codex", agent: "codex", kind: "id", value: thread },
  };
}

test("nested Codex hook cannot replace the parent pane's history or chat target", async () => {
  // A separate codex exec inherits the pane and reports itself; it is not loaded in the pane owner.
  const reported = agent("child-thread");
  const [parent] = await reconcileAgentSessions([reported], async () => ["parent-thread"]);
  expect(parent?.agent_session).toEqual(agent("parent-thread").agent_session);
  expect(parent?.terminal_id).toBe(reported.terminal_id);
  expect(parent?.agent_status).toBe("working");
  expect(parent?.interactive_ready).toBe(true);
  expect(reported.agent_session).toEqual(agent("child-thread").agent_session);
});

test("real thread changes and independently managed child panes keep their own identity", async () => {
  const agents = [agent("new-parent-thread"), agent("child-thread", "child:p1")];
  expect(
    await reconcileAgentSessions(agents, async (pane) =>
      pane === "parent:p1" ? ["old-parent-thread", "new-parent-thread"] : ["child-thread"],
    ),
  ).toEqual(agents);
});

test("unavailable or ambiguous owners never turn an inherited report into a chat target", async () => {
  for (const loaded of [[], ["parent-thread", "other-thread"]]) {
    const [pending] = await reconcileAgentSessions([agent("child-thread")], async () => loaded);
    expect(pending?.agent_session).toBeNull();
    expect(pending?.interactive_ready).toBe(false);
  }
  const [unavailable] = await reconcileAgentSessions([agent("child-thread")], async () => {
    throw new Error("owner temporarily unavailable");
  });
  expect(unavailable?.agent_session).toBeNull();
  expect(unavailable?.terminal_id).toBe("parent-terminal");
});

test("external Codex terminals and other providers retain native discovery", async () => {
  const external = agent("external-thread"),
    claude = { ...agent("claude-thread"), agent: "claude" };
  const inspected: string[] = [];
  expect(
    await reconcileAgentSessions([external, claude], async (pane) => {
      inspected.push(pane);
      return undefined;
    }),
  ).toEqual([external, claude]);
  expect(inspected).toEqual(["parent:p1"]);
});

test("forwarded native errors retain the original request identity and leave state intact", async () => {
  const { binary, state } = herdrFixture(false);
  const response = await herdrRequest(binary, {
    id: "phone-request-17",
    method: "pane.report_agent_session",
    params: { pane_id: "owned:p1", agent_session_id: "wrong-thread" },
  });
  expect(response).toEqual({
    id: "phone-request-17",
    error: {
      code: "invalid_resume_argv",
      message: "resume_argv must start with a plain command name",
    },
  });
  expect(JSON.parse(readFileSync(state, "utf8")).thread).toBe("validation-thread");
});
