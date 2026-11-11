import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  array,
  check,
  json,
  type Model,
  models,
  type NativeAdapter,
  type NativeBlock,
  type NativeOptions,
  type ObjectValue,
  object,
  optionalString,
  parse,
  type RpcMessage,
  string,
} from "@kite/protocol";
import { atomic, entry, readJson, sleep } from "@kite/runtime";
import { RpcError, RpcProcess, TurnCancelled } from "@kite/runtime/rpc";
import { optionalObject, socketRequest } from "@kite/runtime/socket";
import { appendContent } from "../media";
import { permissionSettings, permissionValues } from "../permissions";
export class Dsh implements NativeAdapter {
  models: Model[] = [];
  currentModel = "";
  currentEffort = "";
  readonly path = mkdtempSync(join(tmpdir(), "kite-acp-"));
  private rpc: RpcProcess | undefined;
  private session = "";
  private config: ObjectValue[] = [];
  private image = false;
  private cancelled = false;
  private content = new Map<string, ObjectValue[]>();
  private message = "";
  private tools = new Map<string, ObjectValue>();
  get settings() {
    return permissionSettings(this.config);
  }
  async configure(values: Record<string, string>): Promise<void> {
    for (const [configId, value] of Object.entries(permissionValues(values, this.settings))) {
      const reply = object(
        await this.transport.call("session/set_config_option", {
          sessionId: this.session,
          configId,
          value,
        }),
      );
      this.config = array(reply.configOptions).map(object);
    }
  }
  constructor(private options: NativeOptions) {}
  async initialize(): Promise<this> {
    try {
      await this.start(this.options.route, this.options.model);
      if (!this.options.inspect) {
        const saved = this.options.saved;
        const value = object(
          await this.transport.call(
            saved ? "session/resume" : "session/new",
            json({ ...(saved ? { sessionId: saved } : {}), cwd: this.options.cwd, mcpServers: [] }),
          ),
        );
        this.session = saved ? string(saved) : string(value.sessionId);
        this.config = array(value.configOptions ?? []).map(object);
        this.options.persist(this.session);
      }
      this.currentModel =
        optionalString(this.config.find((o) => o.id === "model")?.currentValue) ||
        JSON.stringify([this.options.route, this.options.model]);
      this.currentEffort = optionalString(
        this.config.find((o) => o.id === "reasoning_effort")?.currentValue,
      );
      for (const row of this.models) row.isDefault = row.model === this.currentModel;
      return this;
    } catch (error) {
      this.close();
      throw error;
    }
  }
  private get transport(): RpcProcess {
    check(this.rpc, "DSH transport unavailable");
    return this.rpc;
  }
  private async start(route: string, model: string): Promise<void> {
    const output = join(this.path, `${crypto.randomUUID()}.json`),
      patch = join(this.path, "profile.patch.json");
    atomic(patch, [
      { id: "acp", config: { provider: route, model } },
      {
        insert: [
          {
            id: "kite-catalog",
            name: join(dirname(entry()), "dsh-plugin.mjs"),
            config: { output, provider: route, control: join(this.path, "control.sock") },
          },
        ],
      },
    ]);
    rmSync(join(this.path, "control.sock"), { force: true });
    this.rpc = new RpcProcess(
      [...this.options.command, "--patch", patch],
      this.options.cwd,
      (v) => this.update(v),
      (v) => this.request(v),
    );
    for (let i = 0; i < 400 && !existsSync(output); i++) {
      check(this.rpc.child.exitCode === null, "DSH exited during startup");
      await sleep(50);
    }
    const hello = object(
      await this.rpc.call(
        "initialize",
        { protocolVersion: 1, clientInfo: { name: "kite", version: "2" }, clientCapabilities: {} },
        15000,
      ),
    );
    check(object(hello.agentInfo).name === "deepseek-harness-acp", "DSH ACP identity mismatch");
    this.image =
      optionalObject(optionalObject(hello.agentCapabilities).promptCapabilities).image === true;
    this.models = models(object(readJson(output)).models);
  }
  private async select(model: string, effort: string): Promise<void> {
    const row = this.models.find((r) => r.model === model);
    check(row, "model unavailable");
    const choices = row.supportedReasoningEfforts.map((e) => e.reasoningEffort);
    check(choices.includes(effort) || (!choices.length && !effort), "reasoning effort unavailable");
    if (row.inputModalities.includes("image") && !this.image) {
      const saved = this.session;
      await this.transport.call("session/close", { sessionId: saved });
      this.transport.close();
      const pair = array(parse(model));
      await this.start(string(pair[0]), string(pair[1]));
      const value = object(
        await this.transport.call("session/resume", {
          sessionId: saved,
          cwd: this.options.cwd,
          mcpServers: [],
        }),
      );
      this.config = array(value.configOptions ?? []).map(object);
      this.currentModel =
        optionalString(this.config.find((o) => o.id === "model")?.currentValue) || model;
      this.session = saved;
    }
    if (model !== this.currentModel) {
      const result = object(
        await this.transport.call("session/set_config_option", {
          sessionId: this.session,
          configId: "model",
          value: model,
        }),
      );
      this.config = array(result.configOptions).map(object);
      this.currentModel = model;
    }
    if (this.config.some((o) => o.id === "reasoning_effort")) {
      const result = object(
        await this.transport.call("session/set_config_option", {
          sessionId: this.session,
          configId: "reasoning_effort",
          value: effort,
        }),
      );
      this.config = array(result.configOptions).map(object);
    }
    this.currentEffort = effort;
  }
  async steer(blocks: NativeBlock[]): Promise<boolean> {
    if (blocks.some((b) => b.type !== "text")) return false;
    let value: JsonResponse;
    try {
      value = object(
        await socketRequest(join(this.path, "control.sock"), {
          method: "steer",
          sessionId: this.session,
          text: blocks
            .filter((b) => b.type === "text")
            .map((b) => b.text)
            .join("\n"),
        }),
      );
    } catch (error) {
      if (
        error instanceof Error &&
        "code" in error &&
        ["ENOENT", "ECONNREFUSED"].includes(String(error.code))
      )
        return false;
      throw error;
    }
    check(!value.error, "native steering failed; delivery may have occurred");
    return value.accepted === true;
  }
  alive(): boolean {
    return this.rpc?.child.exitCode === null;
  }
  prepare(): void {
    this.cancelled = false;
  }
  async run(blocks: NativeBlock[], model: string, effort: string): Promise<undefined> {
    if (this.cancelled) throw new TurnCancelled();
    await this.select(model, effort);
    if (this.cancelled) throw new TurnCancelled();
    this.content.clear();
    this.message = crypto.randomUUID();
    this.tools.clear();
    let value: ObjectValue;
    try {
      value = object(
        await this.transport.call(
          "session/prompt",
          json({ sessionId: this.session, prompt: blocks }),
          3600000,
          () => {
            if (this.cancelled) this.cancel();
          },
        ),
      );
    } catch (error) {
      if (error instanceof RpcError && error.code === -32800) throw new TurnCancelled();
      throw error;
    }
    if (value.stopReason === "cancelled") throw new TurnCancelled();
    check(value.stopReason === "end_turn", "DSH turn failed");
    return undefined;
  }
  cancel(): void {
    this.cancelled = true;
    this.transport.write({ method: "session/cancel", params: { sessionId: this.session } });
  }
  private update(value: RpcMessage): void {
    if (value.method !== "session/update") return;
    const p = object(value.params);
    if (p.sessionId !== this.session) return;
    const u = object(p.update);
    if (u.sessionUpdate === "agent_message_chunk" && u.content) {
      const id = optionalString(u.messageId) || this.message;
      const content = appendContent(this.content.get(id) || [], u.content);
      this.content.set(id, content);
      this.options.event({
        type: "assistant/message",
        seq: id,
        data: { message: { content } },
      });
    } else if (u.sessionUpdate === "tool_call") {
      const id = string(u.toolCallId);
      this.tools.set(id, u);
      this.options.event({
        type: "tool/call",
        data: { callId: id, name: u.title ?? "tool", arguments: JSON.stringify(u.rawInput ?? {}) },
      });
    } else if (
      u.sessionUpdate === "tool_call_update" &&
      ["completed", "failed"].includes(String(u.status))
    ) {
      const output = array(u.content ?? [])
        .map(object)
        .filter((c) => c.type === "content")
        .map((c) => object(c.content));
      this.options.event({
        type: "tool/result",
        data: {
          message: {
            toolCallId: u.toolCallId,
            content: output,
            isError: u.status === "failed",
          },
        },
      });
    }
  }
  private request(value: RpcMessage): void {
    if (value.method === "session/request_permission") {
      const p = object(value.params),
        tool = object(p.toolCall);
      p.toolCall = { ...this.tools.get(String(tool.toolCallId)), ...tool };
      this.options.permission(object(json({ ...value, params: p })));
    } else
      this.transport.write({
        id: value.id ?? null,
        error: { code: -32601, message: "unsupported native request" },
      });
  }
  async approve(request: ObjectValue, accepted: boolean): Promise<void> {
    const p = object(request.params);
    const chosen = array(p.options ?? [])
      .map(object)
      .find((o) => o.kind === (accepted ? "allow_once" : "reject_once"));
    const id = request.id;
    check(typeof id === "string" || typeof id === "number", "invalid permission identity");
    this.transport.write({
      id,
      result: {
        outcome: chosen
          ? { outcome: "selected", optionId: chosen.optionId }
          : { outcome: "cancelled" },
      },
    });
  }
  close(): void {
    this.rpc?.close();
    rmSync(this.path, { recursive: true, force: true });
  }
}
type JsonResponse = ObjectValue;
