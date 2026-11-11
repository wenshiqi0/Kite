import { Database } from "bun:sqlite";
import { appendFileSync, closeSync, existsSync, fsyncSync, openSync, statSync } from "node:fs";
import { join } from "node:path";
import { check, type Json, type ObjectValue, object, optionalString, UUID } from "@kite/protocol";
import { atomic, atomicBytes, home, locked, privateDir, readJson, sha256 } from "@kite/runtime";
import { optionalObject } from "@kite/runtime/socket";
import { stateDir } from "./catalog";

const PUBLIC_TYPES = [
  "userMessage",
  "agentMessage",
  "commandExecution",
  "mcpToolCall",
  "dynamicToolCall",
  "functionCallOutput",
  "fileChange",
  "imageGeneration",
  "imageView",
  "webSearch",
  "collabAgentToolCall",
  "sleep",
];
interface Stored {
  signature: string;
  hashes: Record<string, string>;
  order: string[];
  bytes: number;
}
const row = (id: string, payload: ObjectValue): ObjectValue => ({
  id,
  type: "response_item",
  payload,
});
function content(value: Json | undefined): Json[] {
  if (typeof value === "string") return [{ type: "text", text: value }];
  if (!Array.isArray(value)) return [];
  return value.map((value) => {
    const block = optionalObject(value);
    if (block.type === "inputText") return { type: "text", text: block.text };
    if (block.type === "inputImage")
      return { type: "input_image", image_url: block.imageUrl ?? block.image_url };
    if (block.type === "localImage") return { type: "input_image", image_url: block.path };
    if (block.type === "image" && typeof block.url === "string")
      return { type: "input_image", image_url: block.url };
    if (block.type === "skill")
      return { type: "text", text: `Skill: ${optionalString(block.name)}` };
    return block;
  });
}
/** Convert only public native UI items. Reasoning is excluded in the SQL itself. */
export function nativeRecords(value: ObjectValue): ObjectValue[] {
  const id = optionalString(value.id);
  check(id, "native history item identity unavailable");
  if (value.type === "userMessage")
    return [row(id, { id, type: "message", role: "user", content: content(value.content) })];
  if (value.type === "agentMessage")
    return value.phase === "analysis"
      ? []
      : [
          row(id, {
            id,
            type: "message",
            role: "assistant",
            phase: value.phase,
            content: [{ type: "output_text", text: optionalString(value.text) }],
          }),
        ];
  if (value.type === "imageGeneration")
    return [row(id, { ...value, type: "image_generation_call", saved_path: value.savedPath })];
  if (!PUBLIC_TYPES.includes(optionalString(value.type))) return [];
  let name = optionalString(value.tool || value.name || value.type);
  let argumentsValue: Json | undefined = value.arguments;
  let output: Json | undefined;
  let complete = !["inProgress", "pending", "running"].includes(optionalString(value.status));
  let failed = value.status === "failed" || value.success === false || value.error != null;
  switch (value.type) {
    case "commandExecution":
      name = "exec_command";
      argumentsValue = { command: value.command, cwd: value.cwd };
      output = optionalString(value.aggregatedOutput);
      failed ||= typeof value.exitCode === "number" && value.exitCode !== 0;
      break;
    case "mcpToolCall":
      name = `mcp__${optionalString(value.server)}__${optionalString(value.tool)}`;
      output = optionalObject(value.result).content ?? JSON.stringify(value.error ?? "");
      failed ||= optionalObject(value.result).isError === true;
      break;
    case "dynamicToolCall":
      output = content(value.contentItems);
      break;
    case "functionCallOutput":
      output = value.output;
      break;
    case "imageView":
      name = "view_image";
      argumentsValue = { path: value.path };
      output = [{ type: "image", uri: value.path }];
      break;
    case "fileChange":
      name = "apply_patch";
      argumentsValue = { changes: value.changes };
      output = optionalString(value.status);
      break;
    case "webSearch":
      argumentsValue = { query: value.query, action: value.action };
      output = JSON.stringify(value.results ?? []);
      break;
    default:
      argumentsValue = value;
      output = optionalString(value.status);
      complete = true;
  }
  const result = [
    row(`${id}:call`, {
      type: "function_call",
      call_id: id,
      name,
      arguments:
        typeof argumentsValue === "string" ? argumentsValue : JSON.stringify(argumentsValue ?? {}),
    }),
  ];
  if (complete)
    result.push(
      row(`${id}:output`, {
        type: "function_call_output",
        call_id: id,
        output: output ?? "",
        failed,
      }),
    );
  return result;
}

/** A private append-only projection; provider databases and context are never modified. */
export async function nativeHistoryFile(thread: string): Promise<string | null> {
  check(UUID.test(thread), "invalid native history thread");
  const database = join(
    process.env.CODEX_HOME || join(home(), ".codex"),
    "thread_history_1.sqlite",
  );
  if (!existsSync(database)) return null;
  const root = privateDir(join(stateDir(), "native-history", thread));
  return locked(join(root, "projection.lock"), () => {
    const db = new Database(database, { readonly: true });
    try {
      db.exec("PRAGMA query_only=ON; BEGIN");
      const placeholders = PUBLIC_TYPES.map(() => "?").join(",");
      const where = `thread_id=? AND item_type IN (${placeholders})`;
      const summary = object(
        db
          .query(
            `SELECT count(*) AS n,max(updated_at_ordinal) AS revision,max(rollout_ordinal) AS tip FROM thread_items WHERE ${where}`,
          )
          .get(thread, ...PUBLIC_TYPES),
      );
      if (!summary.n) return null;
      const turns = db
        .query(
          "SELECT turn_id,status,error_json,coalesce(rollout_end_ordinal,rollout_ordinal) AS ordinal FROM thread_turns WHERE thread_id=? ORDER BY ordinal",
        )
        .all(thread)
        .map(object)
        .filter((turn) =>
          ["completed", "failed", "interrupted"].includes(optionalString(turn.status)),
        );
      const signature = sha256(JSON.stringify([summary, turns]));
      const metadata = join(root, "projection.json"),
        file = join(root, "history.jsonl");
      let previous: Stored | undefined;
      if (existsSync(metadata) && existsSync(file)) {
        try {
          const value = object(readJson(metadata));
          if (
            typeof value.signature === "string" &&
            Array.isArray(value.order) &&
            value.hashes &&
            typeof value.hashes === "object" &&
            value.bytes === statSync(file).size
          )
            previous = value as unknown as Stored;
        } catch {
          /* Rebuild damaged metadata from the authoritative database. */
        }
      }
      if (previous?.signature === signature) return file;
      const hashes: Record<string, string> = {},
        order: string[] = [],
        changes: string[] = [];
      const items = db.query(
        `SELECT item_id,item_json,rollout_ordinal FROM thread_items WHERE ${where} ORDER BY rollout_ordinal,created_at_ms,item_id`,
      );
      function terminal(turn: ObjectValue) {
        const payload: ObjectValue = {
          type: turn.status === "interrupted" ? "task_aborted" : "task_complete",
          turn_id: turn.turn_id,
        };
        if (typeof turn.error_json === "string")
          payload.error = JSON.parse(turn.error_json) as Json;
        const record = { type: "event_msg", payload };
        return {
          key: `turn:${turn.turn_id}`,
          source: JSON.stringify(record),
          records: () => [record],
        };
      }
      function* entries() {
        let at = 0;
        for (const record of items.iterate(thread, ...PUBLIC_TYPES)) {
          const stored = object(record),
            id = optionalString(stored.item_id),
            source = optionalString(stored.item_json);
          check(id && source, "invalid native history item");
          while (
            at < turns.length &&
            Number(turns[at]?.ordinal) <= Number(stored.rollout_ordinal)
          ) {
            yield terminal(turns[at] as ObjectValue);
            at++;
          }
          yield {
            key: `item:${id}`,
            source,
            records: () => nativeRecords(object(JSON.parse(source))),
          };
        }
        while (at < turns.length) {
          yield terminal(turns[at] as ObjectValue);
          at++;
        }
      }
      for (const entry of entries()) {
        const hash = sha256(entry.source);
        hashes[entry.key] = hash;
        order.push(entry.key);
        if (previous?.hashes?.[entry.key] !== hash)
          for (const record of entry.records()) changes.push(`${JSON.stringify(record)}\n`);
      }
      const reset = !previous || previous.order.some((id, index) => order[index] !== id);
      if (reset) {
        const all: string[] = !previous ? changes : [];
        if (previous)
          for (const entry of entries())
            for (const record of entry.records()) all.push(`${JSON.stringify(record)}\n`);
        // A native rollback/rewrite starts a new derived inode, never edits the provider.
        atomicBytes(file, Buffer.from(all.join("")));
      } else if (changes.length) {
        const fd = openSync(file, "a", 0o600);
        try {
          appendFileSync(fd, changes.join(""));
          fsyncSync(fd);
        } finally {
          closeSync(fd);
        }
      }
      atomic(metadata, { signature, hashes, order, bytes: statSync(file).size });
      return file;
    } finally {
      db.close();
    }
  });
}
