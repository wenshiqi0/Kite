import { existsSync, realpathSync, statSync } from "node:fs";
import { isAbsolute, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  array,
  check,
  isChatProvider,
  type Json,
  type ObjectValue,
  object,
  optionalString,
  type RpcMessage,
  type SessionRecord,
  type SessionSelection,
  string,
} from "@kite/protocol";
import { home, run, selfCommand } from "@kite/runtime";
import { RpcProcess } from "@kite/runtime/rpc";
import { optionalObject } from "@kite/runtime/socket";
import { sessionOpen } from "./codex";

export interface AcpBackend {
  thread: string;
  model: string;
  effort: string;
  serviceTier?: string;
  models?: ObjectValue[];
  settings?: ObjectValue[];
  status?: string;
  call(method: string, params: Json): Promise<Json>;
  write(message: RpcMessage): void;
  close(): void;
}

export function within(root: string, path: string): boolean {
  const child = relative(root, path);
  return child !== ".." && !child.startsWith("../") && !isAbsolute(child);
}

export class WorkspacePaths {
  private roots: string[];
  constructor(workspaces: string[]) {
    check(
      workspaces.length > 0 && workspaces.length <= 32,
      "ACP requires explicit workspace roots",
    );
    this.roots = workspaces.map((path) => realpathSync(path));
  }
  private resolve(path: string): string {
    check(isAbsolute(path) && !path.split("/").includes(".."), "invalid absolute ACP path");
    let actual: string;
    try {
      actual = realpathSync(path);
    } catch {
      throw new Error("ACP path unavailable");
    }
    check(
      this.roots.some((root) => within(root, actual)),
      "ACP path is outside configured workspaces",
    );
    return actual;
  }
  cwd(path: string): string {
    const actual = this.resolve(path);
    check(statSync(actual).isDirectory(), "ACP working directory must be a directory");
    return actual;
  }
  prompt(value: unknown): ObjectValue[] {
    const blocks = array(value).map(object);
    check(blocks.length > 0 && blocks.length <= 64, "unsupported ACP prompt content");
    return blocks.map((block) => {
      if (block.type === "text") return { type: "text", text: string(block.text) };
      check(block.type === "resource_link", "unsupported ACP prompt content");
      const uri = string(block.uri);
      check(!decodeURIComponent(uri).split("/").includes(".."), "invalid ACP attachment path");
      const path = isAbsolute(uri) ? uri : fileURLToPath(uri);
      const actual = this.resolve(path);
      check(statSync(actual).isFile(), "ACP attachment must be a regular file");
      return { type: "text", text: pathToFileURL(actual).href };
    });
  }
}

export async function verifyRevision(cwd: string, value: unknown): Promise<void> {
  if (value === undefined) return;
  const expected = string(value);
  check(/^[a-f0-9]{40}$/.test(expected), "invalid expected source revision");
  const { stdout, code } = await run(["git", "-C", cwd, "rev-parse", "HEAD"], { timeout: 5000 });
  check(
    code === 0 && stdout.toString().trim() === expected,
    "remote workspace source revision differs from the requested build",
  );
  const dirty = await run(
    [
      "git",
      "-C",
      cwd,
      "-c",
      "core.fsmonitor=false",
      "status",
      "--porcelain",
      "--untracked-files=normal",
    ],
    {
      timeout: 5000,
    },
  );
  check(dirty.code === 0 && !dirty.stdout.length, "remote build workspace is not clean");
}

export function selectModel(
  backend: AcpBackend,
  defaults: SessionSelection,
  requested: SessionSelection,
  codex: boolean,
) {
  const model = requested.model ?? defaults.model ?? backend.model;
  const spec = backend.models?.find((row) => row.model === model);
  check(spec || (!backend.models && model === backend.model), "model unavailable");
  const advertised = Array.isArray(spec?.supportedReasoningEfforts)
    ? spec.supportedReasoningEfforts.map((entry) => object(entry).reasoningEffort)
    : undefined;
  const effort =
    requested.effort ??
    (model === (defaults.model ?? backend.model) ? defaults.effort : undefined) ??
    (model === backend.model && (!advertised || advertised.includes(backend.effort))
      ? backend.effort
      : optionalString(spec?.defaultReasoningEffort));
  if (advertised)
    check(
      advertised.includes(effort) || (!advertised.length && !effort),
      "reasoning effort unavailable",
    );
  else check(effort === backend.effort, "reasoning effort unavailable");
  const tiers = Array.isArray(spec?.serviceTiers)
    ? spec.serviceTiers.map((entry) => object(entry).id)
    : [];
  const configuredTier = backend.serviceTier;
  const chosen =
    requested.serviceTier ??
    (model === (defaults.model ?? backend.model) ? defaults.serviceTier : undefined) ??
    (codex &&
    model === backend.model &&
    (configuredTier === "default" || tiers.includes(configuredTier))
      ? configuredTier
      : undefined);
  if (chosen === undefined) return { model, effort };
  const serviceTier = chosen === "fast" && tiers.includes("priority") ? "priority" : chosen;
  check(
    codex && (serviceTier === "default" || tiers.includes(serviceTier)),
    "model service tier unavailable",
  );
  return { model, effort, serviceTier };
}

export async function connectBackend(
  record: SessionRecord,
  herdr: string,
  event: (message: RpcMessage) => void,
  request: (message: RpcMessage) => void,
): Promise<AcpBackend> {
  const agent = object(object(await sessionOpen(record.id, herdr, false)).agent),
    reference = object(agent.agent_session),
    thread = string(reference.value),
    pane = string(agent.pane_id);
  check(
    reference.kind === "id" &&
      reference.agent === record.provider &&
      isChatProvider(reference.agent),
    "structured owner identity unavailable",
  );
  const rpc = new RpcProcess(
    selfCommand("proxy", "--pane", pane),
    record.cwd || home(),
    event,
    request,
  );
  try {
    await rpc.call("initialize", {
      clientInfo: { name: "kite-acp", version: "1" },
      capabilities: { experimentalApi: true },
    });
    rpc.write({ method: "initialized" });
    const models: ObjectValue[] = [];
    let cursor: string | undefined;
    do {
      const page = object(
        await rpc.call("model/list", { limit: 100, includeHidden: false, cursor }),
      );
      const next = optionalString(page.nextCursor) || undefined;
      const rows = array(page.data).map(object);
      check(
        models.length + rows.length <= 512 && (!next || (rows.length && next !== cursor)),
        "invalid owner model pagination",
      );
      models.push(...rows);
      cursor = next;
    } while (cursor);
    const config = optionalObject(object(await rpc.call("config/read", {})).config),
      model =
        models.find((m) => m.model === config.model) ??
        models.find((m) => m.isDefault === true) ??
        models[0];
    check(model, "owner model unavailable");
    const backend: AcpBackend = {
      thread,
      model: string(model.model),
      effort:
        optionalString(config.model_reasoning_effort) ||
        optionalString(model.defaultReasoningEffort),
      models,
      ...(typeof config.service_tier === "string" ? { serviceTier: config.service_tier } : {}),
      ...(Array.isArray(config.session_settings)
        ? { settings: config.session_settings.map(object) }
        : {}),
      call: (method, params) => rpc.call(method, params),
      write: (message) => rpc.write(message),
      close: () => rpc.close(),
    };
    void rpc.child.exited.then(() =>
      event({ method: "_kite/backend_closed", params: { threadId: thread } }),
    );
    if (record.provider === "codex") {
      const loaded = object(
        object(await backend.call("thread/read", { threadId: thread, includeTurns: false })).thread,
      );
      check(loaded.id === thread, "loaded owner identity mismatch");
      const path = loaded.path;
      if (path === null || (typeof path === "string" && isAbsolute(path) && !existsSync(path))) {
        // Naming persists the empty rollout so resume can subscribe before the first turn.
        await backend.call("thread/name/set", { threadId: thread, name: record.title || "Kite" });
      }
    }
    const resumed = object(
      object(await backend.call("thread/resume", { threadId: thread, excludeTurns: true })).thread,
    );
    check(resumed.id === thread, "resumed owner identity mismatch");
    backend.status = optionalString(optionalObject(resumed.status).type) || "unknown";
    return backend;
  } catch (error) {
    rpc.close();
    throw error;
  }
}
