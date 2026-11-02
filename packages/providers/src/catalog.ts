import { existsSync } from "node:fs";
import { join } from "node:path";
import {
  type Catalog,
  catalog,
  check,
  object,
  SESSION_ID,
  type SessionRecord,
} from "@kite/protocol";
import { atomic, home, locked, privateDir, readJson } from "@kite/runtime";
export const catalogRoot = () => privateDir(join(home(), ".local/share/kite"));
export const stateDir = () => privateDir(join(home(), ".local/state/kite/codex"));
export function catalogUnlocked(): Catalog {
  const file = join(catalogRoot(), "sessions.json");
  return {
    ...(existsSync(file)
      ? catalog(readJson(file))
      : { version: 1 as const, revision: 0, sessions: [] }),
    now_ms: Date.now(),
  };
}
export const catalogRead = () => locked(join(catalogRoot(), "sessions.lock"), catalogUnlocked);
export const sessionLock = (id: string) => {
  check(SESSION_ID.test(id), "invalid session ID");
  return join(catalogRoot(), `session-${id}.lock`);
};
export async function catalogWrite(value: unknown): Promise<Catalog | { conflict: true }> {
  // The Rust Catalog wire DTO omits the on-disk schema version.
  const request = object(value);
  const incoming = catalog({ version: 1, ...request });
  return locked(join(catalogRoot(), "sessions.lock"), () => {
    const current = catalogUnlocked();
    if (current.revision !== incoming.revision) return { conflict: true };
    const next = { ...incoming, revision: current.revision + 1, now_ms: Date.now() };
    atomic(join(catalogRoot(), "sessions.json"), next);
    return next;
  });
}
export async function patchSession(
  id: string,
  fields: Partial<SessionRecord>,
): Promise<SessionRecord> {
  return locked(join(catalogRoot(), "sessions.lock"), () => {
    const value = catalogUnlocked();
    const record = value.sessions.find((r) => r.id === id);
    check(record, "session not found");
    Object.assign(record, fields);
    value.revision++;
    atomic(join(catalogRoot(), "sessions.json"), value);
    return record;
  });
}
