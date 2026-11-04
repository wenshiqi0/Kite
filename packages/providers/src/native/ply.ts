import {
  array,
  check,
  json,
  type Model,
  type NativeAdapter,
  type NativeBlock,
  type NativeOptions,
  type ObjectValue,
  object,
  optionalString,
  type RpcMessage,
  string,
} from "@kite/protocol";
import { RpcProcess, TurnCancelled } from "@kite/runtime/rpc";
import { nativeModelId, optionalObject } from "@kite/runtime/socket";
import { permissionSettings, permissionValues } from "../permissions";

export class Ply implements NativeAdapter {
  models: Model[] = [];
  currentModel = "";
  currentEffort = "";
  readonly rpc: RpcProcess;
  private session = "";
  private cancelled = false;
  private config: ObjectValue[] = [];
  private text = "";
  private message = "";
  private replaying = false;
  private tools = new Map<string, ObjectValue>();
  private permissions = new Set<string | number>();
  get settings() {
    return permissionSettings(this.config);
  }
  async configure(values: Record<string, string>): Promise<void> {
    for (const [configId, value] of Object.entries(permissionValues(values, this.settings))) {
      if (this.config.find((row) => row.id === configId)?.currentValue === value) continue;
      const reply = object(
        await this.rpc.call("session/set_config_option", {
          sessionId: this.session,
          configId,
          value,
        }),
      );
      this.config = array(reply.configOptions).map(object);
    }
  }
  constructor(
    private options: NativeOptions,
    private promptTimeout = 3600000,
  ) {
    this.rpc = new RpcProcess(
      options.command,
      options.cwd,
      (message) => this.update(message),
      (message) => this.request(message),
    );
  }
  async initialize(): Promise<this> {
    try {
      const hello = object(
        await this.rpc.call(
          "initialize",
          {
            protocolVersion: 1,
            clientInfo: { name: "kite", version: "2" },
            clientCapabilities: {},
          },
          15000,
        ),
      );
      check(
        hello.protocolVersion === 1 && object(hello.agentInfo).name === "ply",
        "Ply ACP capability unavailable",
      );
      const metadata = object(object(hello._meta).ply);
      this.models = array(metadata.models).map((value) => {
        const row = object(value);
        return {
          model: string(row.id),
          displayName: string(row.name),
          description: "",
          hidden: false,
          isDefault: nativeModelId(string(row.id)) === this.options.model,
          inputModalities: array(row.input).map(string),
          supportedReasoningEfforts: array(row.efforts).map((value) => ({
            reasoningEffort: string(value),
            description: string(value),
          })),
          defaultReasoningEffort: string(row.defaultEffort),
        };
      });
      const selected = this.models.find((row) => row.isDefault);
      check(selected && this.models.length <= 512, "Ply model catalog unavailable");
      this.currentModel = selected.model;
      this.currentEffort = selected.defaultReasoningEffort;
      if (!this.options.inspect) {
        const saved = this.options.saved;
        check(
          !saved || typeof saved === "string",
          "Ply session predates ACP; start a new ACP session",
        );
        this.replaying = true;
        try {
          const value = object(
            await this.rpc.call(saved ? "session/load" : "session/new", {
              ...(saved ? { sessionId: saved } : {}),
              cwd: this.options.cwd,
              mcpServers: [],
            }),
          );
          this.session = saved ? string(saved) : string(value.sessionId);
          this.config = array(value.configOptions ?? []).map(object);
          this.options.persist(this.session);
        } finally {
          this.replaying = false;
        }
        this.currentModel =
          optionalString(this.config.find((row) => row.id === "model")?.currentValue) ||
          this.currentModel;
        this.currentEffort =
          optionalString(this.config.find((row) => row.id === "reasoning_effort")?.currentValue) ||
          this.currentEffort;
      }
      return this;
    } catch (error) {
      this.close();
      throw error;
    }
  }
  alive(): boolean {
    return this.rpc.child.exitCode === null && this.rpc.child.signalCode === null;
  }
  prepare(): void {
    this.cancelled = false;
  }
  async steer(): Promise<boolean> {
    return false;
  }
  async run(blocks: NativeBlock[], model: string, effort: string): Promise<undefined> {
    if (this.cancelled) throw new TurnCancelled();
    for (const [configId, value] of [
      ["model", model],
      ["reasoning_effort", effort],
    ]) {
      const selected = this.config.find((row) => row.id === configId);
      if (selected?.currentValue === value) continue;
      const reply = object(
        await this.rpc.call("session/set_config_option", {
          sessionId: this.session,
          configId,
          value,
        }),
      );
      this.config = array(reply.configOptions).map(object);
    }
    if (this.cancelled) throw new TurnCancelled();
    this.text = "";
    this.message = crypto.randomUUID();
    this.tools.clear();
    const reply = object(
      await this.rpc.call(
        "session/prompt",
        json({ sessionId: this.session, prompt: blocks }),
        this.promptTimeout,
        () => {
          if (this.cancelled) this.cancel();
        },
        () => this.cancel(),
      ),
    );
    if (reply.stopReason === "cancelled") throw new TurnCancelled();
    check(reply.stopReason === "end_turn" && this.text.trim(), "Ply did not complete its turn");
    this.currentModel = model;
    this.currentEffort = effort;
    return undefined;
  }
  private update(value: RpcMessage): void {
    if (value.method !== "session/update" || this.replaying) return;
    const params = object(value.params);
    if (params.sessionId !== this.session) return;
    const update = object(params.update);
    if (
      update.sessionUpdate === "agent_message_chunk" &&
      optionalObject(update.content).type === "text"
    ) {
      this.text += string(object(update.content).text);
      this.options.event({
        type: "assistant/message",
        seq: this.message,
        data: { message: { content: [{ type: "text", text: this.text }] } },
      });
    } else if (update.sessionUpdate === "config_option_update") {
      this.config = array(update.configOptions).map(object);
    } else if (update.sessionUpdate === "tool_call") {
      const id = string(update.toolCallId);
      this.tools.set(id, update);
      this.options.event({
        type: "tool/call",
        data: {
          callId: id,
          name: update.title ?? "tool",
          arguments: JSON.stringify(update.rawInput ?? {}),
        },
      });
    } else if (
      update.sessionUpdate === "tool_call_update" &&
      ["completed", "failed"].includes(String(update.status))
    ) {
      const output = array(update.content ?? [])
        .map(object)
        .filter((row) => row.type === "content")
        .map((row) => optionalString(optionalObject(row.content).text))
        .join("\n");
      this.options.event({
        type: "tool/result",
        data: {
          message: {
            toolCallId: update.toolCallId,
            content: [{ type: "text", text: output }],
            isError: update.status === "failed",
          },
        },
      });
    }
  }
  private request(value: RpcMessage): void {
    if (value.method !== "session/request_permission") {
      this.rpc.write({
        id: value.id ?? null,
        error: { code: -32601, message: "unsupported native request" },
      });
      return;
    }
    const params = object(value.params),
      tool = object(params.toolCall),
      id = value.id;
    check(typeof id === "string" || typeof id === "number", "invalid permission identity");
    if (params.sessionId !== this.session || this.cancelled) {
      this.rpc.write({ id, result: { outcome: { outcome: "cancelled" } } });
      return;
    }
    params.toolCall = { ...this.tools.get(String(tool.toolCallId)), ...tool };
    this.permissions.add(id);
    this.options.permission(object(json({ ...value, params })));
  }
  async approve(request: ObjectValue, accepted: boolean): Promise<void> {
    const id = request.id;
    check(
      (typeof id === "string" || typeof id === "number") && this.permissions.has(id),
      "permission expired",
    );
    const params = object(request.params);
    check(params.sessionId === this.session, "permission session mismatch");
    const chosen = array(params.options)
      .map(object)
      .find((row) => row.kind === (accepted ? "allow_once" : "reject_once"));
    check(chosen, "permission option unavailable");
    this.permissions.delete(id);
    this.rpc.write({ id, result: { outcome: { outcome: "selected", optionId: chosen.optionId } } });
  }
  cancel(): void {
    this.cancelled = true;
    for (const id of this.permissions)
      this.rpc.write({ id, result: { outcome: { outcome: "cancelled" } } });
    this.permissions.clear();
    if (this.session)
      this.rpc.write({ method: "session/cancel", params: { sessionId: this.session } });
  }
  close(): void {
    this.permissions.clear();
    this.rpc.close();
  }
}
