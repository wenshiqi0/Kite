import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  array,
  check,
  type Json,
  type ObjectValue,
  object,
  SESSION_ID,
  type SessionRecord,
  type SessionSelection,
  sessionSelection,
} from "@kite/protocol";
import { ACP_READ_LIMIT } from "@kite/protocol/acp";
import { atomic, live, privateDir, readJson, startTicks } from "@kite/runtime";
import { catalogRoot, sessionDeleted } from "./catalog";

export const acpStateDir = () => privateDir(join(catalogRoot(), "acp"));
export const wireBytes = (value: Json) => Buffer.byteLength(JSON.stringify(value));
export type TurnResult = {
  turnId: string;
  reply: string;
  tools: ObjectValue[];
  stopReason: string;
  lastTurnStatus: string;
  truncated: boolean;
  updatedAt: string;
  selection?: SessionSelection;
};
const RECENT_TURNS = 8;

export function tailText(text: string, bytes: number): string {
  if (Buffer.byteLength(text) <= bytes) return text;
  const encoded = Buffer.from(text);
  let start = encoded.length - bytes;
  while (start < encoded.length && ((encoded[start] ?? 0) & 0xc0) === 0x80) start++;
  return encoded.subarray(start).toString();
}

/** Work in JSON wire bytes, including quotes, escaping and UTF-8 characters. */
function fitText(text: string, budget: number): string {
  let lo = 0,
    hi = Math.min(Buffer.byteLength(text), budget);
  while (lo < hi) {
    const middle = Math.ceil((lo + hi) / 2);
    if (wireBytes(tailText(text, middle)) - 2 <= budget) lo = middle;
    else hi = middle - 1;
  }
  return tailText(text, lo);
}

export function boundedTurn(
  sessionId: string,
  turn: TurnResult,
  maxBytes: number,
  includeTools: boolean,
) {
  const { tools, reply, selection: _selection, ...meta } = turn;
  const result = {
    sessionId,
    ...meta,
    reply: "",
    ...(includeTools ? { tools: [] as ObjectValue[] } : {}),
  };
  // Reserve the JSON-RPC envelope and request ID within the hard upper bound.
  const budget = Math.max(0, Math.min(maxBytes, ACP_READ_LIMIT - 2048) - wireBytes(result));
  let remaining = budget;
  if (result.tools) {
    let toolBytes = reply ? Math.floor(budget / 3) : budget;
    for (const tool of tools.toReversed()) {
      const base = { ...tool, output: "" };
      const overhead = wireBytes(base) + 1;
      if (overhead > toolBytes) break;
      const output = typeof tool.output === "string" ? tool.output : "";
      const retained = fitText(output, toolBytes - overhead);
      const entry = { ...base, output: retained };
      const bytes = wireBytes(entry) + 1;
      result.tools.unshift(entry);
      toolBytes -= bytes;
      remaining -= bytes;
      result.truncated ||= retained !== output;
    }
    result.truncated ||= result.tools.length !== tools.length;
  }
  result.reply = fitText(reply, remaining);
  result.truncated ||= result.reply !== reply;
  return result;
}

const fileFor = (id: string, suffix: string, directory?: string) => {
  check(SESSION_ID.test(id), "invalid ACP session ID");
  return join(directory ?? acpStateDir(), id + suffix);
};
export function recentTurns(record: SessionRecord, directory?: string): TurnResult[] {
  const path = fileFor(record.id, "-turns.json", directory);
  if (!existsSync(path)) return [];
  return array(readJson(path, RECENT_TURNS * ACP_READ_LIMIT + 4096, true)).map((entry) => {
    const row = object(entry);
    check(
      typeof row.turnId === "string" &&
        typeof row.reply === "string" &&
        typeof row.stopReason === "string" &&
        typeof row.lastTurnStatus === "string" &&
        typeof row.truncated === "boolean" &&
        typeof row.updatedAt === "string",
      "invalid ACP turn projection",
    );
    return {
      turnId: row.turnId,
      reply: row.reply,
      stopReason: row.stopReason,
      lastTurnStatus: row.lastTurnStatus,
      truncated: row.truncated,
      updatedAt: row.updatedAt,
      tools: array(row.tools).map(object),
      ...(row.selection === undefined ? {} : { selection: sessionSelection(row.selection) }),
    };
  });
}
export function saveTurn(record: SessionRecord, turn: TurnResult, directory?: string): void {
  if (directory === undefined && sessionDeleted(record)) return;
  const { sessionId: _, ...bounded } = boundedTurn(record.id, turn, ACP_READ_LIMIT, true);
  atomic(fileFor(record.id, "-turns.json", directory), [
    { ...bounded, selection: turn.selection, tools: bounded.tools ?? [] },
    ...recentTurns(record, directory)
      .filter((entry) => entry.turnId !== turn.turnId)
      .slice(0, RECENT_TURNS - 1),
  ]);
}
export function discardTurnProjection(record: SessionRecord): void {
  if (!existsSync(join(catalogRoot(), "acp"))) return;
  for (const suffix of ["-turns.json", "-interaction.json"]) {
    rmSync(fileFor(record.id, suffix), { force: true });
  }
}

export interface DelegatedInteraction {
  kind: "approval" | "question";
  id: string;
  turnId?: string | undefined;
  command?: string;
  cwd?: string;
  question?: string;
}
export function writeDelegatedInteraction(
  record: SessionRecord,
  pending?: DelegatedInteraction,
  directory?: string,
): void {
  const path = fileFor(record.id, "-interaction.json", directory);
  if (pending) atomic(path, { pid: process.pid, ticks: startTicks(process.pid), pending });
  else rmSync(path, { force: true });
}
export function delegatedInteraction(
  record: SessionRecord,
  directory?: string,
): DelegatedInteraction | undefined {
  const path = fileFor(record.id, "-interaction.json", directory);
  let row: ObjectValue;
  try {
    row = object(readJson(path, ACP_READ_LIMIT, true));
  } catch {
    // The owner may resolve the request while another process is taking a snapshot.
    return;
  }
  const pending = object(row.pending);
  if (
    typeof row.pid !== "number" ||
    typeof row.ticks !== "string" ||
    !live(row.pid, row.ticks) ||
    (pending.kind !== "approval" && pending.kind !== "question") ||
    typeof pending.id !== "string"
  )
    return;
  return {
    kind: pending.kind,
    id: pending.id,
    ...(typeof pending.turnId === "string" ? { turnId: pending.turnId } : {}),
    ...(typeof pending.command === "string" ? { command: pending.command } : {}),
    ...(typeof pending.cwd === "string" ? { cwd: pending.cwd } : {}),
    ...(typeof pending.question === "string" ? { question: pending.question } : {}),
  };
}
