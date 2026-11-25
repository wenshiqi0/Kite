import {
  array,
  check,
  json,
  type NativeBlock,
  object,
  optionalString,
  string,
} from "@kite/protocol";
import { nativeModelId, optionalObject } from "@kite/runtime/socket";
import { acpSystemPromptMeta, kiteMcpServers } from "../kite-context";
import { AcpAdapter } from "./acp";

export class Ply extends AcpAdapter {
  async initialize(): Promise<this> {
    try {
      const hello = await this.handshake("ply");
      const caps = optionalObject(hello.agentCapabilities);
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
        const meta = acpSystemPromptMeta(caps);
        const value = object(
          await this.rpc.call(
            saved ? "session/load" : "session/new",
            json({
              ...(saved ? { sessionId: saved } : {}),
              cwd: this.options.cwd,
              mcpServers: kiteMcpServers(),
              ...(meta ? { _meta: meta } : {}),
            }),
          ),
        );
        this.session = saved ? string(saved) : string(value.sessionId);
        this.readConfig(value.configOptions ?? []);
        this.options.persist(this.session);
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
  async run(blocks: NativeBlock[], model: string, effort: string): Promise<undefined> {
    this.checkCancellation();
    await this.select("model", model);
    await this.select("reasoning_effort", effort);
    await this.prompt(blocks, true);
    this.currentModel = model;
    this.currentEffort = effort;
    return undefined;
  }
}
