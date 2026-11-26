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
  optionalString,
  type RpcMessage,
  string,
} from "@kite/protocol";
import { RpcProcess, TurnCancelled } from "@kite/runtime/rpc";
import { appendContent } from "../media";
import { permissionSettings, permissionValues } from "../permissions";

function toolTitle(value: Json | undefined): string {
  const raw = optionalString(value).trim();
  if (!raw) return "tool";
  const first =
    raw
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find(Boolean) ?? "tool";
  const chars = [...first];
  return chars.length > 160 ? `${chars.slice(0, 159).join("")}…` : first;
}

function diffBlock(row: ObjectValue): ObjectValue | undefined {
  const path = optionalString(row.path).trim();
  const hasOld = typeof row.oldText === "string";
  const hasNew = typeof row.newText === "string";
  if (!path && !hasOld && !hasNew) return undefined;
  const lines: string[] = [];
  if (path) lines.push(`--- ${path}`);
  if (hasOld && row.oldText) {
    for (const line of String(row.oldText).split("\n")) lines.push(`- ${line}`);
  }
  if (hasNew && row.newText) {
    for (const line of String(row.newText).split("\n")) lines.push(`+ ${line}`);
  }
  const text = lines.join("\n");
  return text ? { type: "text", text } : undefined;
}

function toolContentBlocks(value: Json | undefined): ObjectValue[] {
  const blocks: ObjectValue[] = [];
  for (const entry of array(value ?? [])) {
    const row = object(entry);
    if (row.type === "content" && row.content) {
      blocks.push(object(row.content));
    } else if (row.type === "diff") {
      const formatted = diffBlock(row);
      if (formatted) blocks.push(formatted);
    }
  }
  return blocks;
}

/** The stdio, turn and interaction mechanics shared by the ACP providers. */
export abstract class AcpAdapter implements NativeAdapter {
  models: Model[] = [];
  currentModel = "";
  currentEffort = "";
  rpc: RpcProcess;
  protected session = "";
  protected config: ObjectValue[] = [];
  protected readonly splitMessagesAtTools: boolean = false;
  protected supportsSessionClose = false;
  private cancelled = false;
  private promptId: number | undefined;
  private content: ObjectValue[] = [];
  private message = "";
  private tools = new Map<string, ObjectValue>();
  private permissions = new Set<string | number>();

  constructor(
    protected options: NativeOptions,
    private readonly promptTimeout: number | null,
  ) {
    this.rpc = this.connect(options.command);
  }
  private connect(command: string[]): RpcProcess {
    return new RpcProcess(
      command,
      this.options.cwd,
      (message) => this.update(message),
      (message) => this.request(message),
    );
  }
  protected async closeSessionGracefully(): Promise<void> {
    if (!this.supportsSessionClose || !this.session || !this.alive()) return;
    const sessionId = this.session;
    try {
      await this.rpc.call("session/close", { sessionId }, 1500);
    } catch {
      /* best-effort shutdown; process close follows */
    }
  }
  protected async restart(command: string[]): Promise<void> {
    check(this.promptId === undefined, "ACP session is busy");
    await this.closeSessionGracefully();
    const previous = this.rpc;
    previous.close();
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("ACP runtime did not stop")), 5000);
      void previous.child.exited.then(() => {
        clearTimeout(timer);
        resolve();
      });
    });
    this.checkCancellation();
    this.rpc = this.connect(command);
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
    const caps = hello.agentCapabilities ? object(hello.agentCapabilities) : {};
    const sessions = caps.sessionCapabilities ? object(caps.sessionCapabilities) : {};
    this.supportsSessionClose = Boolean(sessions.close);
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
  protected onUpdate(_update: ObjectValue, _sessionId: string): void {}
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
    return this.rpc.alive();
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
    try {
      const reply = object(
        await this.rpc.call(
          "session/prompt",
          json({ sessionId: this.session, prompt: blocks }),
          this.promptTimeout,
          (id) => {
            this.promptId = id;
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
      this.promptId = undefined;
      this.permissions.clear();
    }
  }
  private update(value: RpcMessage): void {
    if (value.method !== "session/update") return;
    const params = object(value.params);
    const sessionId = optionalString(params.sessionId);
    const update = object(params.update);
    if (!this.session) {
      if (sessionId && update.sessionUpdate === "available_commands_update") {
        this.onUpdate(update, sessionId);
      }
      return;
    }
    if (sessionId !== this.session) return;
    if (update.sessionUpdate === "config_option_update") {
      this.readConfig(update.configOptions);
      return;
    }
    this.onUpdate(update, sessionId);
    if (this.promptId === undefined) return;
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
            name: toolTitle(update.title),
            arguments: JSON.stringify(update.rawInput ?? {}),
          },
        });
      }
      const tool = { ...previous, ...update };
      this.tools.set(id, tool);
      if (tool.status !== "completed" && tool.status !== "failed") return;
      const output = toolContentBlocks(tool.content);
      // ACP allows any JSON value in rawOutput. Only objects can carry an exit code.
      const rawOutput = tool.rawOutput;
      this.options.event({
        type: "tool/result",
        data: {
          message: {
            toolCallId: id,
            content: output,
            isError: tool.status === "failed",
            exitCode:
              rawOutput && typeof rawOutput === "object" && !Array.isArray(rawOutput)
                ? rawOutput.exitCode
                : undefined,
          },
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
    if (this.promptId === undefined || params.sessionId !== this.session || this.cancelled) {
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
    if (this.promptId === undefined) return;
    if (this.promptTimeout === null) this.rpc.cancel(this.promptId, () => this.sendCancellation());
    else this.sendCancellation();
  }
  private sendCancellation(): void {
    for (const id of this.permissions)
      this.rpc.write({ id, result: { outcome: { outcome: "cancelled" } } });
    this.permissions.clear();
    this.rpc.write({ method: "session/cancel", params: { sessionId: this.session } });
  }
  close(): void {
    this.permissions.clear();
    if (this.supportsSessionClose && this.session && this.alive()) {
      try {
        this.rpc.write({ method: "session/close", params: { sessionId: this.session } });
      } catch {
        /* best-effort shutdown */
      }
    }
    this.rpc.close();
  }
}
