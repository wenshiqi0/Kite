import {
  array,
  check,
  type Json,
  type Model,
  type NativeBlock,
  type ObjectValue,
  object,
  optionalString,
  type SessionSetting,
  string,
  UUID,
} from "@kite/protocol";
import { optionalObject } from "@kite/runtime/socket";
import { AcpAdapter } from "./acp";

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
  const efforts = new Map<string, Set<string>>();
  for (const row of variants) {
    const variant = string(row.modelId);
    const model = ids.find((id) => variant === id || variant.startsWith(`${id}-`));
    if (!model || model === variant) continue;
    const levels = efforts.get(model) ?? new Set<string>();
    levels.add(variant.slice(model.length + 1));
    efforts.set(model, levels);
  }
  return models.options.map((row) => {
    const isDefault = row.value === models.currentValue;
    const options = isDefault
      ? (thinking?.options ?? [])
      : [...(efforts.get(row.value) ?? [])].map((value) => ({ value, name: value }));
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
        : (options[0]?.value ?? ""),
    };
  });
}

export class Antigravity extends AcpAdapter {
  protected override readonly splitMessagesAtTools = true;
  private hasHistory = false;

  async initialize(): Promise<this> {
    try {
      const hello = await this.handshake("Antigravity ACP Server");
      const caps = object(hello.agentCapabilities),
        sessions = object(caps.sessionCapabilities);
      check(sessions.list && sessions.resume, "Antigravity ACP session capabilities unavailable");
      if (this.options.inspect) {
        // initialize succeeds when signed out. Listing checks the CLI's auth without starting a turn.
        await this.list();
        return this;
      }
      const saved = this.options.saved === undefined ? undefined : object(this.options.saved);
      const sessionId = saved && string(saved.sessionId);
      if (sessionId !== undefined) check(UUID.test(sessionId), "invalid Antigravity session ID");
      const listed = sessionId ? await this.listed(sessionId) : false;
      // agy can resume an unknown ID as an empty session. Avoid silently dropping prior context.
      check(saved?.hasHistory !== true || listed, "saved Antigravity session is missing");
      const result = object(
        await this.rpc.call(
          sessionId ? "session/resume" : "session/new",
          {
            cwd: this.options.cwd,
            mcpServers: [],
            ...(sessionId ? { sessionId } : {}),
          },
          15000,
        ),
      );
      const id = string(result.sessionId);
      check(UUID.test(id) && (!sessionId || id === sessionId), "Antigravity session ID mismatch");
      this.readConfig(result.configOptions);
      const model = this.setting("model");
      check(model, "Antigravity model catalog unavailable");
      this.models = modelCatalog(
        result.models,
        model,
        this.setting("thought_level"),
        optionalObject(caps.promptCapabilities).image === true,
      );
      this.session = id;
      this.persist(listed);
      return this;
    } catch (error) {
      this.close();
      throw error;
    }
  }

  private async list(cursor = ""): Promise<ObjectValue> {
    const result = object(
      await this.rpc.call(
        "session/list",
        { cwd: this.options.cwd, ...(cursor ? { cursor } : {}) },
        4000,
      ),
    );
    array(result.sessions);
    return result;
  }
  private async listed(sessionId: string): Promise<boolean> {
    let cursor = "";
    for (let page = 0; page < 4; page++) {
      const result = await this.list(cursor);
      if (array(result.sessions).some((row) => object(row).sessionId === sessionId)) return true;
      const next = optionalString(result.nextCursor);
      if (!next || next === cursor) return false;
      cursor = next;
    }
    return false;
  }
  private persist(restorable: boolean): void {
    this.hasHistory ||= restorable;
    this.options.persist({ sessionId: this.session, hasHistory: this.hasHistory, restorable });
  }

  private setting(id: string): SessionSetting | undefined {
    const row = this.config.find((value) => value.id === id);
    return row && selection(row);
  }
  override get settings(): SessionSetting[] {
    const mode = this.setting("mode");
    return mode ? [mode] : [];
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
      await this.prompt(blocks);
    } finally {
      // Only release an archived owner after ACP can find its native context.
      this.persist(await this.listed(this.session).catch(() => false));
    }
    return undefined;
  }
}
