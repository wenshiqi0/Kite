import { existsSync } from "node:fs";
import { join } from "node:path";
import {
  type Catalog,
  catalog,
  catalogEnvelope,
  check,
  isChatProvider,
  type ObjectValue,
  object,
  SESSION_ID,
  type SessionRecord,
  session,
} from "@kite/protocol";
import { atomic, home, locked, privateDir, readJson, sha256 } from "@kite/runtime";
import { codexThread, setCodexArchived } from "./codex-storage";
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
function diskCatalog() {
  const file = join(catalogRoot(), "sessions.json");
  const value = existsSync(file)
    ? catalogEnvelope(readJson(file))
    : { version: 1 as const, revision: 0, sessions: [] };
  const sessions: SessionRecord[] = [];
  const foreign: ObjectValue[] = [];
  const ids = new Set<string>();
  for (const entry of value.sessions) {
    const row = object(entry);
    check(
      typeof row.id === "string" && SESSION_ID.test(row.id) && !ids.has(row.id),
      "invalid or duplicate session ID",
    );
    ids.add(row.id);
    if (row.provider === "claude" || isChatProvider(row.provider)) sessions.push(session(row));
    else {
      check(
        typeof row.provider === "string" && /^[a-z][a-z0-9_-]{0,63}$/.test(row.provider),
        "invalid provider",
      );
      foreign.push(row);
    }
  }
  return { version: value.version, revision: value.revision, sessions, foreign };
}
export function saveCatalogUnlocked(value: Catalog): Catalog {
  const next = {
    ...value,
    revision: value.revision + 1,
    now_ms: Date.now(),
    sessions: value.sessions.filter((record) => !sessionDeleted(record)),
  };
  // Preserve future provider rows as opaque JSON, even across native CAS writes.
  const { foreign } = diskCatalog();
  const reserved = new Set(foreign.map((row) => row.id));
  check(next.sessions.length + foreign.length <= 1024, "session catalog is full");
  for (const record of next.sessions) {
    check(!reserved.has(record.id), "duplicate or reserved session ID");
    reserved.add(record.id);
  }
  const disk = { ...next, sessions: [...next.sessions, ...foreign] };
  check(Buffer.byteLength(JSON.stringify(disk)) <= 1024 * 1024, "session catalog is full");
  atomic(join(catalogRoot(), "sessions.json"), disk);
  return next;
}
export function catalogUnlocked(): Catalog {
  const { revision, sessions } = diskCatalog();
  return {
    version: 1,
    revision,
    sessions: sessions.filter((record) => !sessionDeleted(record)),
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
  return locked(join(catalogRoot(), "sessions.lock"), async () => {
    const current = catalogUnlocked();
    if (current.revision !== incoming.revision) return { conflict: true };
    const previousById = new Map(current.sessions.map((record) => [record.id, record]));
    for (const record of incoming.sessions) {
      const previous = previousById.get(record.id);
      // Lineage and remote defaults are assigned by the gateway, not by a
      // client updating history, display metadata, or archive state.
      if (previous?.delegation) record.delegation = previous.delegation;
      else delete record.delegation;
      if (previous?.acp_selection) record.acp_selection = previous.acp_selection;
      else delete record.acp_selection;
    }
    const archived = new Map(current.sessions.filter((r) => r.archived).map((r) => [r.id, r]));
    for (const record of incoming.sessions) {
      const previous = archived.get(record.id),
        thread = codexThread(record);
      if (!record.archived && previous && thread && thread === codexThread(previous))
        await setCodexArchived(thread, false);
    }
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
    saveCatalogUnlocked(value);
    return record;
  });
}
