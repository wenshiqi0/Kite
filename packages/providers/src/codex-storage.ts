import { existsSync } from "node:fs";
import { join } from "node:path";
import { check, type SessionRecord, UUID } from "@kite/protocol";
import { home, run } from "@kite/runtime";
import { historyFiles } from "./history";

export function codexThread(record: SessionRecord): string | undefined {
  const ref = record.reference;
  if (
    record.provider === "codex" &&
    ref?.agent === "codex" &&
    ref.kind === "id" &&
    UUID.test(ref.value)
  )
    return ref.value;
  return undefined;
}

export function codexHistoryFiles(thread: string, archived = false): string[] {
  check(UUID.test(thread), "invalid Codex session reference");
  const root = join(
    process.env.CODEX_HOME || join(home(), ".codex"),
    archived ? "archived_sessions" : "sessions",
  );
  return existsSync(root) ? historyFiles(root, thread) : [];
}

export async function setCodexArchived(thread: string, archived: boolean): Promise<boolean> {
  const from = codexHistoryFiles(thread, !archived);
  if (!from.length) return false;
  check(
    from.length === 1 && !codexHistoryFiles(thread, archived).length,
    "ambiguous Codex session history",
  );
  await run(["codex", archived ? "archive" : "unarchive", thread], {
    env: process.env,
    timeout: 3000,
  });
  check(
    codexHistoryFiles(thread, archived).length === 1 &&
      !codexHistoryFiles(thread, !archived).length,
    "Codex session archive state could not be verified",
  );
  return true;
}
