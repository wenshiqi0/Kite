import { closeSync, constants, fstatSync, openSync, readSync, realpathSync } from "node:fs";
import { isAbsolute, join, relative } from "node:path";
import {
  array,
  check,
  type Json,
  type ObjectValue,
  object,
  optionalString,
  UUID,
} from "@kite/protocol";
import { home } from "@kite/runtime";

export interface Subagent extends ObjectValue {
  id: string;
  parent_id: string;
  name: string;
  task: string;
  runtime_status: string;
  active_flags: string[];
  last_outcome: string | null;
  updated_ms: number;
}
type ReadRpc = (method: string, params: Json) => Promise<Json>;
const MAX_AGENTS = 256;

/** Publish native facts; Rust owns the shared activity/outcome rules. */
export function subagentFacts(thread: ObjectValue): {
  runtime_status: string;
  active_flags: string[];
  last_outcome: string | null;
} {
  const status = thread.status ? object(thread.status) : {};
  return {
    runtime_status: optionalString(status.type),
    active_flags: Array.isArray(status.activeFlags)
      ? status.activeFlags.filter((v): v is string => typeof v === "string")
      : [],
    last_outcome:
      status.type === "active" || status.type === "systemError"
        ? null
        : (lastOutcome(optionalString(thread.path)) ?? null),
  };
}

/** Read only a bounded, complete tail of an owned Codex rollout; never publish its text. */
function lastOutcome(path: string): string | undefined {
  if (!isAbsolute(path)) return;
  let fd: number | undefined;
  try {
    const root = process.env.CODEX_HOME || join(home(), ".codex");
    const file = realpathSync(path);
    if (
      !["sessions", "archived_sessions"].some((directory) => {
        const rel = relative(join(root, directory), file);
        return rel !== "" && rel !== ".." && !rel.startsWith("../") && !isAbsolute(rel);
      })
    )
      return;
    fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    const info = fstatSync(fd);
    if (!info.isFile() || info.uid !== process.getuid?.()) return;
    const start = Math.max(0, info.size - 128 * 1024),
      bytes = Buffer.alloc(info.size - start);
    const n = readSync(fd, bytes, 0, bytes.length, start);
    const lines = bytes.subarray(0, n).toString().split("\n");
    lines.pop(); // Ignore a partially written final record.
    if (start) lines.shift();
    for (const line of lines.reverse()) {
      try {
        const row = object(JSON.parse(line) as unknown);
        if (row.type !== "event_msg") continue;
        const event = object(row.payload);
        if (event.type === "task_started") return;
        if (event.type === "task_complete") return event.error == null ? "completed" : "failed";
        if (event.type === "turn_aborted" || event.type === "task_aborted") return "aborted";
      } catch {
        // A damaged line is not a terminal outcome.
      }
    }
  } catch {
    // History may have moved during archive, or may not have been persisted yet.
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
  return undefined;
}

/** Native descendant discovery is metadata-only: no resume, subscription or model turn. */
export async function codexSubagents(parent: string, rpc: ReadRpc): Promise<Subagent[]> {
  check(UUID.test(parent), "invalid parent thread");
  const threads = new Map<string, ObjectValue>();
  for (const archived of [false, true]) {
    let cursor: string | null = null;
    const cursors = new Set<string>();
    for (let page = 0; page < 4; page++) {
      const result = object(
        await rpc("thread/list", {
          ancestorThreadId: parent,
          sourceKinds: ["subAgentThreadSpawn"],
          modelProviders: [],
          useStateDbOnly: true,
          sortKey: "created_at",
          sortDirection: "asc",
          archived,
          limit: 100,
          cursor,
        }),
      );
      for (const value of array(result.data)) {
        const thread = object(value);
        if (typeof thread.id === "string" && UUID.test(thread.id) && thread.id !== parent)
          threads.set(thread.id, thread);
      }
      cursor = typeof result.nextCursor === "string" ? result.nextCursor : null;
      if (!cursor) break;
      check(!cursors.has(cursor), "repeated subagent page");
      cursors.add(cursor);
      check(page < 3, "subagent discovery exceeds page limit");
    }
  }
  const children = new Map<string, Subagent[]>();
  for (const [id, thread] of threads) {
    const source = thread.source && typeof thread.source === "object" ? object(thread.source) : {};
    const sub =
      source.subAgent && typeof source.subAgent === "object" ? object(source.subAgent) : {};
    const spawn =
      sub.thread_spawn && typeof sub.thread_spawn === "object" ? object(sub.thread_spawn) : {};
    const parentId =
      optionalString(thread.parentThreadId) || optionalString(spawn.parent_thread_id);
    if (!UUID.test(parentId) || parentId === id) continue;
    const task = optionalString(spawn.agent_path)
      .replace(/^\/root\/?/, "")
      .slice(0, 256);
    const name = (
      optionalString(thread.agentNickname) ||
      optionalString(spawn.agent_nickname) ||
      task.split("/").pop() ||
      id.slice(0, 8)
    ).slice(0, 128);
    const entry: Subagent = {
      id,
      parent_id: parentId,
      name,
      task,
      ...subagentFacts(thread),
      updated_ms: typeof thread.updatedAt === "number" ? Math.trunc(thread.updatedAt * 1000) : 0,
    };
    const siblings = children.get(parentId) || [];
    siblings.push(entry);
    children.set(parentId, siblings);
  }
  const result: Subagent[] = [],
    seen = new Set([parent]);
  const visit = (id: string, depth: number): void => {
    check(depth <= 32, "subagent tree exceeds depth limit");
    for (const child of children.get(id) || []) {
      if (seen.has(child.id)) continue;
      check(result.length < MAX_AGENTS, "subagent tree exceeds size limit");
      seen.add(child.id);
      result.push(child);
      visit(child.id, depth + 1);
    }
  };
  visit(parent, 0);
  return result;
}

export async function attachSubagents(
  agents: ObjectValue[],
  read: (pane: string, thread: string) => Promise<Subagent[] | undefined>,
): Promise<ObjectValue[]> {
  return Promise.all(
    agents.map(async (agent) => {
      const ref = agent.agent_session ? object(agent.agent_session) : {};
      if (
        agent.agent !== "codex" ||
        typeof agent.pane_id !== "string" ||
        ref.kind !== "id" ||
        typeof ref.value !== "string"
      )
        return agent;
      try {
        const children = await read(agent.pane_id, ref.value);
        return children === undefined
          ? agent
          : { ...agent, subagents: children, subagents_error: false };
      } catch {
        return { ...agent, subagents: null, subagents_error: true };
      }
    }),
  );
}
