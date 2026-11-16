import { Database } from "bun:sqlite";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { object, parse } from "@kite/protocol";
import { sha256 } from "@kite/runtime";
import { historyProjection } from "../src/history";
import { nativeHistoryFile, nativeRecords } from "../src/native-history";

let root = "",
  previous: string | undefined,
  db: Database;
const thread = "12345678-1234-1234-1234-123456789abc";
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "kite-native-history-"));
  previous = process.env.HOME;
  process.env.HOME = root;
  mkdirSync(join(root, ".codex"));
  db = new Database(join(root, ".codex/thread_history_1.sqlite"));
  db.exec(
    "CREATE TABLE thread_items(thread_id TEXT,item_id TEXT,item_type TEXT,item_json TEXT,rollout_ordinal INTEGER,updated_at_ordinal INTEGER,created_at_ms INTEGER); CREATE TABLE thread_turns(thread_id TEXT,turn_id TEXT,status TEXT,error_json TEXT,rollout_ordinal INTEGER,rollout_end_ordinal INTEGER)",
  );
});
afterEach(() => {
  db.close();
  if (previous === undefined) delete process.env.HOME;
  else process.env.HOME = previous;
  rmSync(root, { recursive: true, force: true });
});
function item(id: string, type: string, ordinal: number, fields: Record<string, unknown>) {
  db.query("INSERT INTO thread_items VALUES(?,?,?,?,?,?,?)").run(
    thread,
    id,
    type,
    JSON.stringify({ id, type, ...fields }),
    ordinal,
    ordinal,
    ordinal,
  );
}
function display(data: Buffer) {
  const first = data.indexOf(10),
    second = data.indexOf(10, first + 1),
    size = Number(data.subarray(first + 1, second).toString());
  return data
    .subarray(second + 1 + size)
    .toString()
    .trim()
    .split("\n")
    .map((line) => object(parse(line)));
}
test("native paginated text and tool history is read without a JSONL rollout or private reasoning", async () => {
  item("u", "userMessage", 1, { content: [{ type: "text", text: "owned question" }] });
  item("private", "reasoning", 2, { text: "PRIVATE_REASONING_DO_NOT_EXPORT" });
  item("cmd", "commandExecution", 3, {
    command: "printf owned",
    cwd: "/owned",
    status: "completed",
    aggregatedOutput: "owned output",
    exitCode: 0,
  });
  item("a", "agentMessage", 4, { text: "Substantive native answer", phase: "final_answer" });
  db.query("INSERT INTO thread_turns VALUES(?,?,?,?,?,?)").run(
    thread,
    "turn-1",
    "completed",
    null,
    1,
    5,
  );
  const database = join(root, ".codex/thread_history_1.sqlite"),
    before = sha256(readFileSync(database));
  const file = await nativeHistoryFile(thread);
  expect(file).not.toBeNull();
  expect(existsSync(join(root, ".codex/sessions"))).toBe(false);
  const source = readFileSync(file as string, "utf8");
  expect(source).not.toContain("PRIVATE_REASONING");
  expect(source).toContain("Substantive native answer");
  expect(source).toContain("owned output");
  const rows = display(await historyProjection(file as string));
  expect(rows).toHaveLength(5);
  expect(sha256(readFileSync(database))).toBe(before);
});
test("new replies and changed native items append with stable identity, source inode and unchanged-read stamp", async () => {
  item("a", "agentMessage", 1, { text: "first", phase: "final_answer" });
  const file = (await nativeHistoryFile(thread)) as string,
    initial = statSync(file),
    initialBytes = readFileSync(file);
  expect(await nativeHistoryFile(thread)).toBe(file);
  expect(statSync(file).mtimeMs).toBe(initial.mtimeMs);
  db.query("UPDATE thread_items SET item_json=?,updated_at_ordinal=2 WHERE item_id='a'").run(
    JSON.stringify({
      id: "a",
      type: "agentMessage",
      text: "first completed",
      phase: "final_answer",
    }),
  );
  item("next", "agentMessage", 3, { text: "next substantive reply", phase: "final_answer" });
  await nativeHistoryFile(thread);
  expect(statSync(file).ino).toBe(initial.ino);
  expect(readFileSync(file).subarray(0, initialBytes.length)).toEqual(initialBytes);
  expect(readFileSync(file, "utf8")).toContain("next substantive reply");
});
test("rollback resets only the derived projection and terminal errors remain in source order", async () => {
  item("first", "agentMessage", 1, { text: "first", phase: "final_answer" });
  db.query("INSERT INTO thread_turns VALUES(?,?,?,?,?,?)").run(
    thread,
    "failed-turn",
    "failed",
    JSON.stringify({ message: "owned failure" }),
    1,
    2,
  );
  item("second", "agentMessage", 3, { text: "second", phase: "final_answer" });
  const file = (await nativeHistoryFile(thread)) as string;
  const rows = readFileSync(file, "utf8")
    .trim()
    .split("\n")
    .map((line) => object(parse(line)));
  expect(rows.map((row) => object(row.payload).type)).toEqual([
    "message",
    "task_complete",
    "message",
  ]);
  const inode = statSync(file).ino;
  db.query("DELETE FROM thread_items WHERE item_id='second'").run();
  await nativeHistoryFile(thread);
  expect(statSync(file).ino).not.toBe(inode);
  expect(readFileSync(file, "utf8")).not.toContain('"id":"second"');
});
test("missing native stores remain pending and invalid identities never select arbitrary files", async () => {
  expect(await nativeHistoryFile(thread)).toBeNull();
  await expect(nativeHistoryFile("../outside")).rejects.toThrow("invalid native history thread");
  expect(nativeRecords({ id: "private", type: "reasoning", text: "private" })).toEqual([]);
  expect(
    nativeRecords({ id: "analysis", type: "agentMessage", phase: "analysis", text: "private" }),
  ).toEqual([]);
});
test("native public tool DTOs preserve their output, errors and image content", () => {
  for (const value of [
    {
      id: "mcp",
      type: "mcpToolCall",
      server: "owned",
      tool: "read",
      status: "completed",
      arguments: {},
      result: { content: [{ type: "text", text: "MCP output" }] },
    },
    {
      id: "dynamic",
      type: "dynamicToolCall",
      tool: "read",
      status: "completed",
      arguments: {},
      success: true,
      contentItems: [{ type: "inputText", text: "dynamic output" }],
    },
    { id: "image", type: "imageView", path: "/owned/image.png" },
  ]) {
    const rows = nativeRecords(value);
    expect(rows).toHaveLength(2);
    expect(object(rows[1]?.payload).type).toBe("function_call_output");
  }
});
