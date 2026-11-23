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
const MANAGED_PROVIDERS = ["dsh", "ply", "antigravity"] as const;
export type ManagedProvider = (typeof MANAGED_PROVIDERS)[number];
export const PROVIDERS = ["codex", "claude", ...MANAGED_PROVIDERS] as const;
export type Provider = (typeof PROVIDERS)[number];
export const CHAT_PROVIDERS = ["codex", ...MANAGED_PROVIDERS] as const;
export type ChatProvider = (typeof CHAT_PROVIDERS)[number];

export function isManagedProvider(value: unknown): value is ManagedProvider {
  return MANAGED_PROVIDERS.some((provider) => provider === value);
}

export function isChatProvider(value: unknown): value is ChatProvider {
  return value === "codex" || isManagedProvider(value);
}
export interface Reference {
  source: string;
  agent: string;
  kind: string;
  value: string;
}
export const DELEGATION_POLICIES = ["strict", "human_in_the_loop", "isolated_auto"] as const;
export type DelegationPolicy = (typeof DELEGATION_POLICIES)[number];
export const REMOTE_SESSION_STATES = [
  "idle",
  "working",
  "waiting_approval",
  "waiting_question",
  "failed",
  "released",
] as const;
export type RemoteSessionState = (typeof REMOTE_SESSION_STATES)[number];
export interface SessionParent {
  machine: string;
  sessionId: string;
  provider?: string;
}
export interface SessionDelegation {
  parent?: SessionParent;
  label?: string;
  role?: string;
  policy: DelegationPolicy;
  writeLock?: boolean;
  worktreeIsolated?: boolean;
}
export interface SessionSelection {
  model?: string;
  effort?: string;
  serviceTier?: string;
}
export function boundedString(value: unknown, limit: number): string {
  const text = string(value);
  const chars = [...text];
  check(
    text.trim() &&
      chars.length <= limit &&
      chars.every((ch) => ch.charCodeAt(0) >= 32 && ch.charCodeAt(0) !== 127),
    "invalid bounded string",
  );
  return text;
}
export function delegationPolicy(value: unknown): DelegationPolicy {
  const policy = DELEGATION_POLICIES.find((entry) => entry === value);
  check(policy, "invalid delegation policy");
  return policy;
}
export function remoteSessionState(value: unknown): RemoteSessionState {
  const state = REMOTE_SESSION_STATES.find((entry) => entry === value);
  check(state, "invalid remote session state");
  return state;
}
export function boolean(value: unknown): boolean {
  check(typeof value === "boolean", "boolean required");
  return value;
}
export function sessionParent(value: unknown): SessionParent {
  const row = object(value);
  return {
    machine: boundedString(row.machine, 128),
    sessionId: boundedString(row.sessionId, 128),
    ...(row.provider === undefined ? {} : { provider: boundedString(row.provider, 64) }),
  };
}
export function sessionDelegation(value: unknown): SessionDelegation {
  const row = object(value);
  return {
    policy: delegationPolicy(row.policy),
    ...(row.parent === undefined ? {} : { parent: sessionParent(row.parent) }),
    ...(row.label === undefined ? {} : { label: boundedString(row.label, 120) }),
    ...(row.role === undefined ? {} : { role: boundedString(row.role, 64) }),
    ...(row.writeLock === undefined ? {} : { writeLock: boolean(row.writeLock) }),
    ...(row.worktreeIsolated === undefined
      ? {}
      : { worktreeIsolated: boolean(row.worktreeIsolated) }),
  };
}
export function sessionSelection(value: unknown): SessionSelection {
  const row = object(value);
  return {
    ...(row.model === undefined ? {} : { model: boundedString(row.model, 256) }),
    ...(row.effort === undefined ? {} : { effort: boundedString(row.effort, 64) }),
    ...(row.serviceTier === undefined ? {} : { serviceTier: boundedString(row.serviceTier, 64) }),
  };
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
  delegation?: SessionDelegation;
  acp_selection?: SessionSelection;
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
  check(provider === "claude" || isChatProvider(provider), "invalid provider");
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
  if (row.delegation !== undefined) result.delegation = sessionDelegation(row.delegation);
  if (row.acp_selection !== undefined) result.acp_selection = sessionSelection(row.acp_selection);
  return result;
}
export function catalogEnvelope(value: unknown) {
  const row = object(value);
  check(row.version === 1, "unsupported session catalog");
  check(integer(row.revision) >= 0, "invalid catalog revision");
  const rows = array(row.sessions);
  check(rows.length <= 1024, "session capacity exceeded");
  return { version: 1 as const, revision: integer(row.revision), sessions: rows };
}
export function catalog(value: unknown): Catalog {
  const envelope = catalogEnvelope(value);
  return { ...envelope, sessions: envelope.sessions.map(session) };
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
  account?(): Promise<ObjectValue>;
  rateLimits?(): Promise<ObjectValue>;
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
