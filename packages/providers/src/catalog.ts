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
import { atomic, home, locked, privateDir, readJson, sha256 } from "@kite/runtime";
export const catalogRoot = () => privateDir(join(home(), ".local/share/kite"));
export const stateDir = () => privateDir(join(home(), ".local/state/kite/codex"));
function deletionMarkers(record: SessionRecord): string[] {
  check(SESSION_ID.test(record.id), "invalid session ID");
  const names = [`id-${record.id}`];
  const ref = record.reference;
  if (ref)
    names.push(`ref-${sha256(JSON.stringify([ref.source, ref.agent, ref.kind, ref.value]))}`);
  return names.map((name) => join(catalogRoot(), "deleted-sessions", name));
}
/** Markers contain no conversation content and fence stale discovery from every client. */
export const sessionDeleted = (record: SessionRecord) => deletionMarkers(record).some(existsSync);
export function markSessionDeleted(record: SessionRecord): void {
  privateDir(join(catalogRoot(), "deleted-sessions"));
  // The reference marker goes first: interrupted writes must also fence rediscovery under a new ID.
  for (const path of deletionMarkers(record).reverse()) atomic(path, {});
}
export function saveCatalogUnlocked(value: Catalog): Catalog {
  const next = {
    ...value,
    revision: value.revision + 1,
    now_ms: Date.now(),
    sessions: value.sessions.filter((record) => !sessionDeleted(record)),
  };
  atomic(join(catalogRoot(), "sessions.json"), next);
  return next;
}
export function catalogUnlocked(): Catalog {
  const file = join(catalogRoot(), "sessions.json");
  const value = existsSync(file)
    ? catalog(readJson(file))
    : { version: 1 as const, revision: 0, sessions: [] };
  return {
    ...value,
    sessions: value.sessions.filter((record) => !sessionDeleted(record)),
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
    return saveCatalogUnlocked(incoming);
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
