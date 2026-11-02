import { afterAll, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { reportSessionBinding } from "../src/codex";

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
