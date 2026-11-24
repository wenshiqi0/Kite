import {
  array,
  check,
  type Json,
  type Model,
  type NativeBlock,
  type NativeOptions,
  type ObjectValue,
  object,
  optionalString,
  parse,
  type SessionSetting,
  string,
  UUID,
} from "@kite/protocol";
import { run, sleep } from "@kite/runtime";
import { optionalObject } from "@kite/runtime/socket";
import { permissionValues } from "../permissions";
import { AcpAdapter } from "./acp";

const SESSION_LIST_TIMEOUT_MS = 12_000;
const USAGE_TIMEOUT_MS = 20_000;
const SKIP_PERMISSIONS = "--dangerously-skip-permissions";
const COMMAND_WAIT_MS = 400;
const MAX_COMMANDS = 512;

interface AntigravitySkill {
  name: string;
  description: string;
  path: string;
  enabled: true;
}

function validCommandName(name: string): boolean {
  if (!name || name.length > 256) return false;
  for (const ch of name) {
    const code = ch.charCodeAt(0);
    if (code <= 32 || code === 127) return false;
  }
  return true;
}

function parseCommands(value: Json | undefined): AntigravitySkill[] {
  const seen = new Set<string>();
  const skills: AntigravitySkill[] = [];
  for (const entry of array(value ?? [])) {
    if (skills.length >= MAX_COMMANDS) break;
    const row = object(entry);
    const name = optionalString(row.name).trim();
    if (!validCommandName(name) || seen.has(name)) continue;
    seen.add(name);
    skills.push({
      name,
      description: optionalString(row.description).trim(),
      path: `/${name}`,
      enabled: true,
    });
  }
  return skills;
}

function extractEffortLabel(name: string): string | undefined {
  const match = /\(([^()]+)\)\s*$/.exec(name.trim());
  return match?.[1]?.trim() || undefined;
}

function promptWithSkills(blocks: NativeBlock[]): NativeBlock[] {
  const skills = blocks.filter(
    (block): block is Extract<NativeBlock, { type: "skill" }> => block.type === "skill",
  );
  if (!skills.length) return blocks;
  const mentions = skills.map((skill) => `$${skill.name}`).join(" ");
  const slashMentions = skills.map((skill) => `/${skill.name}`).join(" ");
  let rewritten = false;
  const output: NativeBlock[] = [];
  for (const block of blocks) {
    if (block.type === "skill") continue;
    if (!rewritten && block.type === "text") {
      rewritten = true;
      if (block.text === mentions) {
        output.push({ type: "text", text: slashMentions });
        continue;
      }
      if (block.text.startsWith(`${mentions}\n\n`)) {
        output.push({
          type: "text",
          text: `${slashMentions}\n\n${block.text.slice(mentions.length + 2)}`,
        });
        continue;
      }
      output.push({
        type: "text",
        text: block.text.trim() ? `${slashMentions}\n\n${block.text}` : slashMentions,
      });
      continue;
    }
    output.push(block);
  }
  if (!rewritten) output.unshift({ type: "text", text: slashMentions });
  return output;
}

function permissionCommand(command: string[], turbo: boolean): string[] {
  return [
    ...command.filter((arg) => arg !== SKIP_PERMISSIONS),
    ...(turbo ? [SKIP_PERMISSIONS] : []),
  ];
}

function usageCommand(command: string[]): string[] {
  return [
    ...command.filter((arg) => arg !== "acp" && arg !== SKIP_PERMISSIONS),
    "-p",
    "/usage",
    "--output-format",
    "json",
  ];
}

function windowDurationMins(window: string): number | undefined {
  const value = window.trim().toLowerCase();
  if (value === "weekly") return 7 * 24 * 60;
  if (value === "daily") return 24 * 60;
  const hours = /^(\d+)h$/.exec(value);
  if (hours) return Number(hours[1]) * 60;
  const days = /^(\d+)d$/.exec(value);
  if (days) return Number(days[1]) * 24 * 60;
  const minutes = /^(\d+)m$/.exec(value);
  if (minutes) return Number(minutes[1]);
  return undefined;
}

function resetSeconds(resetTime: string): number | undefined {
  if (!resetTime) return undefined;
  const ms = Date.parse(resetTime);
  if (!Number.isFinite(ms) || ms <= 0) return undefined;
  return Math.floor(ms / 1000);
}

function groupId(name: string, bucketIds: string[]): string {
  for (const raw of bucketIds) {
    const trimmed = raw.replace(/-(?:weekly|daily|\d+[hdm])$/i, "").trim();
    if (trimmed) return trimmed;
  }
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "antigravity";
}

function unavailableReason(message: string): "unsupportedAuth" | "noAccount" | undefined {
  if (/gemini api key|not available with/i.test(message)) return "unsupportedAuth";
  if (/not logged in|authentication required|sign in|unauthenticated/i.test(message))
    return "noAccount";
  return undefined;
}

function decodeUsageResponse(stdout: string, stderr: string, code: number): ObjectValue {
  const trimmed = stdout.trim();
  let payload: ObjectValue | undefined;
  if (trimmed) {
    try {
      payload = object(parse(trimmed));
    } catch {
      /* handled below */
    }
  }
  const errorText = optionalString(payload?.error) || stderr.trim();
  if (code !== 0 || payload?.status === "ERROR") {
    const unavailable = unavailableReason(errorText);
    if (unavailable) return { rateLimits: {}, unavailable };
    throw new Error("Antigravity quota read failed");
  }
  check(payload && payload.status === "SUCCESS", "invalid Antigravity quota response");
  const command = object(payload.command);
  check(command.name === "usage", "unexpected Antigravity quota command");
  const data = object(command.data);
  const groups = array(data.groups ?? []).map(object);
  check(groups.length <= 64, "Antigravity quota groups exceed limit");
  const buckets: ObjectValue[] = [];
  for (const group of groups) {
    const name = optionalString(group.name).trim();
    const rawBuckets = array(group.buckets ?? []).map(object);
    check(rawBuckets.length <= 32, "Antigravity quota buckets exceed limit");
    const parsed = rawBuckets
      .map((row) => {
        const id = optionalString(row.id).trim();
        const label = optionalString(row.name).trim();
        const window = optionalString(row.window).trim();
        const resetTime = optionalString(row.reset_time).trim();
        let fraction: number;
        if (typeof row.remaining_fraction === "number") {
          fraction = row.remaining_fraction;
        } else if (row.remaining_fraction === undefined && (window || resetTime || id)) {
          // Go json omitempty drops remaining_fraction when the quota is exhausted (0).
          fraction = 0;
        } else {
          throw new Error("invalid Antigravity quota fraction");
        }
        check(
          Number.isFinite(fraction) && fraction >= 0 && fraction <= 1.001,
          "invalid Antigravity quota fraction",
        );
        const remaining = Math.round(Math.min(1, Math.max(0, fraction)) * 100);
        const duration = windowDurationMins(window);
        const resetsAt = resetSeconds(resetTime);
        return {
          id,
          label,
          exhausted: remaining === 0,
          duration: duration ?? Number.MAX_SAFE_INTEGER,
          window: {
            usedPercent: 100 - remaining,
            ...(duration === undefined ? {} : { windowDurationMins: duration }),
            ...(resetsAt === undefined ? {} : { resetsAt }),
          },
        };
      })
      .sort((a, b) => a.duration - b.duration);
    if (parsed.length === 0) continue;
    const baseId = groupId(
      name,
      parsed.map((item) => item.id),
    );
    for (let index = 0; index < parsed.length; index += 2) {
      const slice = parsed.slice(index, index + 2);
      const first = slice[0];
      const second = slice[1];
      if (!first) continue;
      const limitId = index === 0 ? baseId : `${baseId}-${index / 2 + 1}`;
      const limitName = name || first.label || limitId;
      const reached = slice.some((item) => item.exhausted);
      buckets.push({
        limitId,
        limitName,
        primary: first.window,
        ...(second ? { secondary: second.window } : {}),
        ...(reached ? { rateLimitReachedType: "quota_exhausted" } : {}),
      });
    }
  }
  return {
    rateLimits: buckets[0] ?? {},
    buckets,
  };
}

export async function readAntigravityUsage(command: string[], cwd: string): Promise<ObjectValue> {
  const { stdout, stderr, code } = await run(usageCommand(command), {
    cwd,
    timeout: USAGE_TIMEOUT_MS,
    allowFailure: true,
  });
  return decodeUsageResponse(stdout.toString(), stderr.toString(), code);
}

function selection(row: ObjectValue): SessionSetting {
  check(row.type === "select", "invalid Antigravity configuration");
  const choices = array(row.options).map(object);
  const options = choices
    .flatMap((choice) => (choice.options ? array(choice.options).map(object) : [choice]))
    .map((choice) => ({ value: string(choice.value), name: string(choice.name) }));
  const currentValue = string(row.currentValue);
  check(
    options.length <= 512 && options.some((option) => option.value === currentValue),
    "invalid Antigravity configuration choices",
  );
  return { id: string(row.id), name: string(row.name), currentValue, options };
}

function modelCatalog(
  advertised: Json | undefined,
  models: SessionSetting,
  thinking: SessionSetting | undefined,
  image: boolean,
): Model[] {
  // ACP exposes all model/effort variants and the live effort selector for the current base model.
  const ids = models.options.map((row) => row.value).sort((a, b) => b.length - a.length);
  const variants = array(optionalObject(advertised).availableModels ?? []).map(object);
  check(variants.length <= 2048, "Antigravity model catalog exceeds limit");
  const efforts = new Map<string, Map<string, string>>();
  for (const row of variants) {
    const variant = string(row.modelId);
    const model = ids.find((id) => variant === id || variant.startsWith(`${id}-`));
    if (!model || model === variant) continue;
    const effort = variant.slice(model.length + 1);
    const levels = efforts.get(model) ?? new Map<string, string>();
    if (!levels.has(effort)) {
      levels.set(effort, extractEffortLabel(optionalString(row.name)) ?? effort);
    }
    efforts.set(model, levels);
  }
  const preferredDefault = (options: { value: string; name: string }[]): string => {
    for (const candidate of [thinking?.currentValue, "medium", "high"]) {
      if (candidate && options.some((option) => option.value === candidate)) return candidate;
    }
    return options[0]?.value ?? "";
  };
  return models.options.map((row) => {
    const isDefault = row.value === models.currentValue;
    const options = isDefault
      ? (thinking?.options ?? [])
      : [...(efforts.get(row.value) ?? [])].map(([value, name]) => ({ value, name }));
    return {
      model: row.value,
      displayName: row.name,
      description: "",
      hidden: false,
      isDefault,
      inputModalities: image ? ["text", "image"] : ["text"],
      supportedReasoningEfforts: options.map(({ value, name }) => ({
        reasoningEffort: value,
        description: name,
      })),
      defaultReasoningEffort: isDefault
        ? (thinking?.currentValue ?? "")
        : preferredDefault(options),
    };
  });
}

export class Antigravity extends AcpAdapter {
  protected override readonly splitMessagesAtTools = true;
  private hasHistory = false;
  private turbo: boolean;
  private plan: string | undefined;
  private supportsLoad = false;
  private commandsBySession = new Map<string, AntigravitySkill[]>();
  private usageRequest: Promise<ObjectValue> | undefined;

  constructor(options: NativeOptions) {
    const saved = options.saved === undefined ? undefined : object(options.saved);
    const policy = saved?.toolPermission;
    check(
      policy === undefined || policy === "default" || policy === "turbo",
      "invalid tool permissions",
    );
    const turbo = options.inspect
      ? options.command.includes(SKIP_PERMISSIONS)
      : policy === undefined
        ? true
        : policy === "turbo";
    super({ ...options, command: permissionCommand(options.command, turbo) });
    this.turbo = turbo;
  }

  async initialize(): Promise<this> {
    try {
      this.session = "";
      this.commandsBySession.clear();
      const hello = await this.handshake("Antigravity ACP Server");
      const caps = object(hello.agentCapabilities),
        sessions = object(caps.sessionCapabilities);
      check(sessions.list && sessions.resume, "Antigravity ACP session capabilities unavailable");
      this.supportsLoad = caps.loadSession === true || Boolean(sessions.load);
      if (this.options.inspect) {
        // initialize succeeds when signed out. Listing checks the CLI's auth without starting a turn.
        await this.list();
        return this;
      }
      const saved = this.options.saved === undefined ? undefined : object(this.options.saved);
      const sessionId = saved && string(saved.sessionId);
      if (sessionId !== undefined) check(UUID.test(sessionId), "invalid Antigravity session ID");
      const listed = sessionId ? await this.listed(sessionId) : false;
      const useLoad = Boolean(
        sessionId && saved?.hasHistory === true && !listed && this.supportsLoad,
      );
      // agy without session/load can resume an unknown ID as an empty session.
      // When session/load is available, use it to verify and recover sessions evicted from session/list.
      check(
        saved?.hasHistory !== true || listed || useLoad,
        "saved Antigravity session is missing",
      );
      const method = useLoad ? "session/load" : sessionId ? "session/resume" : "session/new";
      const result = object(
        await this.rpc.call(
          method,
          {
            cwd: this.options.cwd,
            mcpServers: [],
            ...(sessionId ? { sessionId } : {}),
          },
          15000,
        ),
      );
      const id =
        useLoad && result.sessionId === undefined ? string(sessionId) : string(result.sessionId);
      check(UUID.test(id) && (!sessionId || id === sessionId), "Antigravity session ID mismatch");
      this.readConfig(result.configOptions);
      const model = this.setting("model");
      check(model, "Antigravity model catalog unavailable");
      this.plan =
        optionalString(
          optionalObject(optionalObject(optionalObject(result._meta).antigravity).tier).name,
        ).trim() || undefined;
      this.models = modelCatalog(
        result.models,
        model,
        this.setting("thought_level"),
        optionalObject(caps.promptCapabilities).image === true,
      );
      this.session = id;
      this.persist(listed || useLoad);
      return this;
    } catch (error) {
      this.close();
      throw error;
    }
  }

  protected override onUpdate(update: ObjectValue, sessionId: string): void {
    if (update.sessionUpdate === "available_commands_update") {
      this.commandsBySession.set(sessionId, parseCommands(update.availableCommands));
    }
  }

  private async list(cursor = "", timeout = SESSION_LIST_TIMEOUT_MS): Promise<ObjectValue> {
    const result = object(
      await this.rpc.call(
        "session/list",
        { cwd: this.options.cwd, ...(cursor ? { cursor } : {}) },
        timeout,
      ),
    );
    array(result.sessions);
    return result;
  }
  private async listed(sessionId: string): Promise<boolean> {
    const deadline = performance.now() + SESSION_LIST_TIMEOUT_MS;
    let cursor = "";
    for (let page = 0; page < 20; page++) {
      const remaining = deadline - performance.now();
      if (remaining <= 0) return false;
      const result = await this.list(cursor, remaining);
      if (array(result.sessions).some((row) => object(row).sessionId === sessionId)) return true;
      const next = optionalString(result.nextCursor);
      if (!next || next === cursor) return false;
      cursor = next;
    }
    return false;
  }
  private persist(restorable: boolean): void {
    this.hasHistory ||= restorable;
    const saved = {
      sessionId: this.session,
      hasHistory: this.hasHistory,
      restorable,
      toolPermission: this.turbo ? "turbo" : "default",
    };
    this.options.saved = saved;
    this.options.persist(saved);
  }

  private setting(id: string): SessionSetting | undefined {
    const row = this.config.find((value) => value.id === id);
    return row && selection(row);
  }
  override get settings(): SessionSetting[] {
    const mode = this.setting("mode");
    return [
      ...(mode ? [mode] : []),
      {
        id: "tool_permission",
        name: "Tool permissions",
        currentValue: this.turbo ? "turbo" : "default",
        options: [
          { value: "turbo", name: "Turbo (auto-approve tools)" },
          { value: "default", name: "Use host settings" },
        ],
      },
    ];
  }
  async account(): Promise<ObjectValue> {
    return {
      account: { type: "antigravity", ...(this.plan ? { planType: this.plan } : {}) },
      requiresOpenaiAuth: false,
    };
  }
  async rateLimits(): Promise<ObjectValue> {
    if (!this.usageRequest) {
      this.usageRequest = readAntigravityUsage(this.options.command, this.options.cwd).finally(
        () => {
          this.usageRequest = undefined;
        },
      );
    }
    return this.usageRequest;
  }
  async skills(cwds: string[]): Promise<ObjectValue> {
    if (this.session && !this.commandsBySession.has(this.session)) {
      const deadline = performance.now() + COMMAND_WAIT_MS;
      while (
        this.session &&
        !this.commandsBySession.has(this.session) &&
        performance.now() < deadline
      ) {
        await sleep(25);
      }
    }
    const items = (this.session ? this.commandsBySession.get(this.session) : undefined) ?? [];
    return {
      data: cwds.map((cwd) => ({
        cwd,
        skills: items.map((item) => ({ ...item })),
        errors: [],
      })),
    };
  }
  override async configure(values: Record<string, string>): Promise<void> {
    const selected = permissionValues(values, this.settings);
    const mode = selected.mode ?? this.setting("mode")?.currentValue;
    if (
      selected.tool_permission !== undefined &&
      (selected.tool_permission === "turbo") !== this.turbo
    ) {
      // agy exposes this policy as a launch flag, not an ACP config option. Resume
      // the same native conversation between turns; never approve pending requests.
      const turbo = selected.tool_permission === "turbo";
      await this.restart(permissionCommand(this.options.command, turbo));
      this.turbo = turbo;
      await this.initialize();
    }
    this.checkCancellation();
    if (mode) await this.select("mode", mode);
  }
  protected override readConfig(value: Json | undefined): void {
    super.readConfig(value);
    this.currentModel = this.setting("model")?.currentValue ?? "";
    this.currentEffort = this.setting("thought_level")?.currentValue ?? "";
  }
  protected override permissionTool(tool: ObjectValue): ObjectValue {
    const input = optionalObject(tool.rawInput);
    return tool.kind === "execute" && typeof input.CommandLine === "string"
      ? { ...tool, rawInput: { ...input, command: input.CommandLine } }
      : tool;
  }
  async run(blocks: NativeBlock[], model: string, effort: string): Promise<undefined> {
    this.checkCancellation();
    await this.select("model", model);
    check(this.currentModel === model, "Antigravity model selection did not apply");
    if (effort) {
      check(
        this.setting("thought_level")?.options.some((option) => option.value === effort),
        "Antigravity thinking level unavailable",
      );
      await this.select("thought_level", effort);
    }
    this.checkCancellation();
    try {
      await this.prompt(promptWithSkills(blocks));
    } finally {
      // Only release an archived owner after ACP can find its native context.
      this.persist(await this.listed(this.session).catch(() => false));
    }
    return undefined;
  }
}
