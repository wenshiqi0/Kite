import {
  closeSync,
  constants,
  existsSync,
  fstatSync,
  openSync,
  readSync,
  realpathSync,
} from "node:fs";
import { dirname, join } from "node:path";
import {
  type Json,
  type ObjectValue,
  optionalString,
  parse,
  type SessionRecord,
} from "@kite/protocol";
import { home } from "@kite/runtime";
import { optionalObject } from "@kite/runtime/socket";
import { within } from "./acp-backend";
import { type TurnResult, tailText } from "./acp-state";
import { codexHistoryFiles, codexThread } from "./codex-storage";
import { harnessDirectory } from "./harness";

const HISTORY_LIMIT = 4 * 1024 * 1024;

function source(record: SessionRecord): string | undefined {
  const thread = codexThread(record);
  if (thread) {
    const paths = [false, true].flatMap((archived) => codexHistoryFiles(thread, archived));
    const path = paths.length === 1 ? paths[0] : undefined;
    if (!path) return;
    const root = process.env.CODEX_HOME || join(home(), ".codex");
    const allowed = ["sessions", "archived_sessions"].map((folder) => join(root, folder));
    if (allowed.some((base) => existsSync(base) && within(realpathSync(base), realpathSync(path))))
      return path;
  } else if (record.reference?.source === "kite:harness") {
    const path = join(harnessDirectory(record.reference.value), "history.jsonl");
    if (existsSync(path) && realpathSync(dirname(path)) === dirname(realpathSync(path)))
      return path;
  }
  return undefined;
}

function publicText(value: Json | undefined): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value))
    return value
      .map((entry) => optionalString(optionalObject(entry).text))
      .filter(Boolean)
      .join("\n");
  return "";
}

/** Recover completed public output from a bounded native history window, without opening an owner. */
export function historyTurn(record: SessionRecord, requested?: string): TurnResult | undefined {
  const path = source(record);
  if (!path) return;
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  let data: string, mtime: number, offset: number;
  try {
    const stat = fstatSync(fd);
    if (!stat.isFile()) return;
    const bytes = Buffer.alloc(Math.min(stat.size, HISTORY_LIMIT));
    offset = stat.size - bytes.length;
    data = bytes.subarray(0, readSync(fd, bytes, 0, bytes.length, offset)).toString();
    mtime = stat.mtimeMs;
  } finally {
    closeSync(fd);
  }
  const lines = data.split("\n");
  lines.pop(); // Ignore a partially appended line.
  if (offset) lines.shift();
  let current: string | undefined,
    result: TurnResult | undefined,
    reply = "";
  const tools = new Map<string, ObjectValue>();
  for (const line of lines) {
    let entry: ObjectValue;
    try {
      entry = optionalObject(parse(line));
    } catch {
      continue;
    }
    const payload = optionalObject(entry.payload);
    if (entry.type === "event_msg") {
      const id = optionalString(payload.turn_id);
      if (!id) continue;
      if (payload.type === "task_started") {
        current = id;
        reply = "";
        tools.clear();
      } else if (
        (payload.type === "task_complete" || payload.type === "task_aborted") &&
        (!requested || requested === id)
      ) {
        const matching = current === id;
        const final = optionalString(payload.last_agent_message);
        const status =
          payload.type === "task_aborted"
            ? "interrupted"
            : payload.failed || payload.error
              ? "failed"
              : "completed";
        result = {
          turnId: id,
          reply: final || (matching ? reply : ""),
          tools: matching ? [...tools.values()] : [],
          stopReason:
            status === "failed" ? "failed" : status === "interrupted" ? "cancelled" : "end_turn",
          lastTurnStatus: status,
          truncated: !matching || !!offset,
          updatedAt: new Date(mtime).toISOString(),
        };
      }
      continue;
    }
    if (entry.type !== "response_item" || !current || (requested && current !== requested))
      continue;
    if (
      payload.type === "message" &&
      payload.role === "assistant" &&
      payload.channel !== "analysis" &&
      payload.phase !== "analysis"
    ) {
      const text = publicText(payload.content);
      reply = tailText(reply + text, HISTORY_LIMIT);
    } else if (payload.type === "function_call" && typeof payload.call_id === "string") {
      let name = optionalString(payload.name);
      try {
        const args = optionalObject(parse(optionalString(payload.arguments)));
        name = optionalString(args.command) || optionalString(args.cmd) || name;
      } catch {
        /* The original tool name is still available. */
      }
      if (tools.size >= 24) {
        const oldest = tools.keys().next().value;
        if (oldest) tools.delete(oldest);
      }
      tools.set(payload.call_id, {
        id: tailText(payload.call_id, 128),
        type: "tool",
        command: tailText(name, 2048),
        status: "in_progress",
      });
    } else if (payload.type === "function_call_output" && typeof payload.call_id === "string") {
      const tool = tools.get(payload.call_id);
      if (!tool) continue;
      tool.status =
        payload.failed === true || (typeof payload.exitCode === "number" && payload.exitCode !== 0)
          ? "failed"
          : "completed";
      tool.output = tailText(publicText(payload.output), 32 * 1024);
      if (typeof payload.exitCode === "number") tool.exitCode = payload.exitCode;
    }
  }
  return result;
}
