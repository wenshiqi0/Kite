/** JSON enters packages as unknown and is checked before use. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json | undefined };
export type ObjectValue = { [key: string]: Json | undefined };
export function object(value: unknown): ObjectValue {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    throw new Error("object required");
  return value as ObjectValue;
}
export function array(value: unknown): Json[] {
  if (!Array.isArray(value)) throw new Error("array required");
  return value as Json[];
}
export function string(value: unknown): string {
  if (typeof value !== "string") throw new Error("string required");
  return value;
}
export function integer(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value))
    throw new Error("safe integer required");
  return value;
}
export function parse(text: string): Json {
  return JSON.parse(text) as Json;
}
export function json(value: unknown): Json {
  return parse(JSON.stringify(value));
}
export function optionalString(value: unknown): string {
  return typeof value === "string" ? value : "";
}
export function check(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
export const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
export const SESSION_ID = /^[a-f0-9]{32,64}$/;
export const MAX_FILE = 25 * 1024 * 1024;
export const RPC_LIMIT = 40 * 1024 * 1024;
export type Provider = "codex" | "claude" | "dsh" | "ply";
export type ManagedProvider = "dsh" | "ply";
export interface Reference {
  source: string;
  agent: string;
  kind: string;
  value: string;
}
export interface SessionRecord {
  id: string;
  provider: Provider;
  title: string;
  cwd?: string | null;
  reference?: Reference | null;
  terminal_id?: string | null;
  pane_id?: string | null;
  workspace_id?: string | null;
  agent_name?: string | null;
  archived?: boolean;
  [key: string]: unknown;
}
export interface Catalog {
  version: 1;
  revision: number;
  sessions: SessionRecord[];
  now_ms?: number;
}
export function reference(value: unknown): Reference {
  const r = object(value);
  return {
    source: string(r.source),
    agent: string(r.agent),
    kind: string(r.kind),
    value: string(r.value),
  };
}
export function session(value: unknown): SessionRecord {
  const row = object(value),
    id = string(row.id),
    provider = string(row.provider),
    title = string(row.title);
  check(SESSION_ID.test(id), "invalid session ID");
  check(
    provider === "codex" || provider === "claude" || provider === "dsh" || provider === "ply",
    "invalid provider",
  );
  const result: SessionRecord = { ...row, id, provider, title };
  for (const key of ["cwd", "terminal_id", "pane_id", "workspace_id", "agent_name"] as const) {
    const value = row[key];
    if (value !== undefined) result[key] = value === null ? null : string(value);
  }
  if (row.reference !== undefined) {
    result.reference = row.reference === null ? null : reference(row.reference);
    if (result.reference)
      check(result.reference.agent === provider, "session reference provider mismatch");
  }
  if (row.archived !== undefined) {
    check(typeof row.archived === "boolean", "invalid archive state");
    result.archived = row.archived;
  }
  return result;
}
export function catalog(value: unknown): Catalog {
  const row = object(value);
  check(row.version === 1, "unsupported session catalog");
  check(integer(row.revision) >= 0, "invalid catalog revision");
  const rows = array(row.sessions);
  check(rows.length <= 1024, "session capacity exceeded");
  return { version: 1, revision: integer(row.revision), sessions: rows.map(session) };
}
export interface Model {
  model: string;
  displayName: string;
  description: string;
  hidden: boolean;
  isDefault: boolean;
  inputModalities: string[];
  supportedReasoningEfforts: { reasoningEffort: string; description: string }[];
  defaultReasoningEffort: string;
}
export function models(value: unknown): Model[] {
  const rows = array(value);
  check(rows.length > 0 && rows.length <= 512, "invalid model catalog");
  return rows.map((value) => {
    const r = object(value);
    return {
      model: string(r.model),
      displayName: string(r.displayName),
      description: optionalString(r.description),
      hidden: r.hidden === true,
      isDefault: r.isDefault === true,
      inputModalities: array(r.inputModalities).map(string),
      defaultReasoningEffort: string(r.defaultReasoningEffort),
      supportedReasoningEfforts: array(r.supportedReasoningEfforts).map((e) => {
        const entry = object(e);
        return {
          reasoningEffort: string(entry.reasoningEffort),
          description: optionalString(entry.description),
        };
      }),
    };
  });
}
export type NativeBlock =
  | { type: "text"; text: string }
  | { type: "image"; mimeType: string; data: string };
export interface RpcMessage {
  jsonrpc?: string;
  id?: string | number | null;
  method?: string;
  params?: Json;
  result?: Json;
  error?: { code: number; message: string };
}
export interface NativeAdapter {
  models: Model[];
  currentModel: string;
  currentEffort: string;
  settings?: SessionSetting[];
  configure?(values: Record<string, string>): Promise<void>;
  alive(): boolean;
  prepare(): void;
  run(blocks: NativeBlock[], model: string, effort: string): Promise<string | undefined>;
  steer(blocks: NativeBlock[]): Promise<boolean>;
  cancel(): void;
  approve(request: ObjectValue, accepted: boolean): Promise<void>;
  close(): void;
}
export interface SessionSetting {
  id: string;
  name: string;
  currentValue: string;
  options: { value: string; name: string }[];
}
export interface NativeOptions {
  command: string[];
  cwd: string;
  model: string;
  route: string;
  saved?: Json;
  inspect?: boolean;
  event: (event: ObjectValue) => void;
  permission: (request: ObjectValue) => void;
  persist: (value: Json) => void;
}

export function rpcMessage(value: unknown): RpcMessage {
  const row = object(value),
    message: RpcMessage = {};
  if (row.id !== undefined) {
    check(
      row.id === null || typeof row.id === "string" || typeof row.id === "number",
      "invalid RPC ID",
    );
    message.id = row.id;
  }
  if (row.jsonrpc !== undefined) message.jsonrpc = string(row.jsonrpc);
  if (row.method !== undefined) message.method = string(row.method);
  if (row.params !== undefined) message.params = row.params;
  if (row.result !== undefined) message.result = row.result;
  if (row.error !== undefined) {
    const error = object(row.error);
    message.error = { code: integer(error.code), message: string(error.message) };
  }
  return message;
}
