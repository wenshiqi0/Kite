import {
  array,
  check,
  type Json,
  json,
  type Model,
  type NativeAdapter,
  type NativeBlock,
  type NativeOptions,
  type ObjectValue,
  object,
  type RpcMessage,
  string,
} from "@kite/protocol";
import { RpcProcess, TurnCancelled } from "@kite/runtime/rpc";
import { appendContent } from "../media";
import { permissionSettings, permissionValues } from "../permissions";

/** The stdio, turn and interaction mechanics shared by the ACP providers. */
export abstract class AcpAdapter implements NativeAdapter {
  models: Model[] = [];
  currentModel = "";
  currentEffort = "";
  readonly rpc: RpcProcess;
  protected session = "";
  protected config: ObjectValue[] = [];
  protected readonly splitMessagesAtTools: boolean = false;
  private cancelled = false;
  private prompting = false;
  private content: ObjectValue[] = [];
  private message = "";
  private tools = new Map<string, ObjectValue>();
  private permissions = new Set<string | number>();

  constructor(
    protected options: NativeOptions,
    private promptTimeout = 3600000,
  ) {
    this.rpc = new RpcProcess(
      options.command,
      options.cwd,
      (message) => this.update(message),
      (message) => this.request(message),
    );
  }
  abstract run(blocks: NativeBlock[], model: string, effort: string): Promise<undefined>;

  protected async handshake(agentName: string): Promise<ObjectValue> {
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
      hello.protocolVersion === 1 && object(hello.agentInfo).name === agentName,
      `${agentName} capability unavailable`,
    );
    return hello;
  }
  get settings() {
    return permissionSettings(this.config);
  }
  async configure(values: Record<string, string>): Promise<void> {
    for (const [configId, value] of Object.entries(permissionValues(values, this.settings)))
      await this.select(configId, value);
  }
  protected readConfig(value: Json | undefined): void {
    this.config = array(value).map(object);
  }
  protected async select(configId: string, value: string): Promise<void> {
    if (this.config.find((row) => row.id === configId)?.currentValue === value) return;
    const reply = object(
      await this.rpc.call("session/set_config_option", {
        sessionId: this.session,
        configId,
        value,
      }),
    );
    this.readConfig(reply.configOptions);
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
  protected checkCancellation(): void {
    if (this.cancelled) throw new TurnCancelled();
  }
  private startMessage(): void {
    this.content = [];
    this.message = crypto.randomUUID();
  }
  protected async prompt(blocks: NativeBlock[], requireReply = false): Promise<void> {
    this.checkCancellation();
    this.startMessage();
    this.tools.clear();
    this.prompting = true;
    try {
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
      check(
        reply.stopReason === "end_turn" && (!requireReply || this.content.length > 0),
        "ACP agent did not complete its turn",
      );
    } finally {
      this.prompting = false;
      this.permissions.clear();
    }
  }
  private update(value: RpcMessage): void {
    if (value.method !== "session/update") return;
    const params = object(value.params);
    if (!this.session || params.sessionId !== this.session) return;
    const update = object(params.update);
    if (update.sessionUpdate === "config_option_update") {
      this.readConfig(update.configOptions);
      return;
    }
    if (!this.prompting) return;
    if (update.sessionUpdate === "agent_message_chunk" && update.content) {
      const content = object(update.content);
      this.content = appendContent(this.content, content);
      this.options.event({
        type: "assistant/message",
        seq: this.message,
        data: {
          message: { content: this.content },
          delta: content.type === "text" ? string(content.text) : "",
        },
      });
    } else if (
      update.sessionUpdate === "tool_call" ||
      update.sessionUpdate === "tool_call_update"
    ) {
      const id = string(update.toolCallId),
        previous = this.tools.get(id);
      if (previous?.status === "completed" || previous?.status === "failed") return;
      if (!previous) {
        if (update.sessionUpdate !== "tool_call") return;
        if (this.splitMessagesAtTools && this.content.length > 0) this.startMessage();
        this.options.event({
          type: "tool/call",
          data: {
            callId: id,
            name: update.title ?? "tool",
            arguments: JSON.stringify(update.rawInput ?? {}),
          },
        });
      }
      const tool = { ...previous, ...update };
      this.tools.set(id, tool);
      if (tool.status !== "completed" && tool.status !== "failed") return;
      const output = array(tool.content ?? [])
        .map(object)
        .filter((row) => row.type === "content")
        .map((row) => object(row.content));
      this.options.event({
        type: "tool/result",
        data: {
          message: { toolCallId: id, content: output, isError: tool.status === "failed" },
        },
      });
    }
  }
  protected permissionTool(tool: ObjectValue): ObjectValue {
    return tool;
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
      id = value.id;
    check(typeof id === "string" || typeof id === "number", "invalid permission identity");
    if (!this.prompting || params.sessionId !== this.session || this.cancelled) {
      this.rpc.write({ id, result: { outcome: { outcome: "cancelled" } } });
      return;
    }
    const tool = object(params.toolCall);
    params.toolCall = this.permissionTool({ ...this.tools.get(String(tool.toolCallId)), ...tool });
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
    this.rpc.write({
      id,
      result: { outcome: { outcome: "selected", optionId: string(chosen.optionId) } },
    });
  }
  cancel(): void {
    this.cancelled = true;
    for (const id of this.permissions)
      this.rpc.write({ id, result: { outcome: { outcome: "cancelled" } } });
    this.permissions.clear();
    if (this.prompting)
      this.rpc.write({ method: "session/cancel", params: { sessionId: this.session } });
  }
  close(): void {
    this.permissions.clear();
    this.rpc.close();
  }
}
