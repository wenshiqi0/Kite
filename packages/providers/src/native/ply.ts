import { closeSync, existsSync, mkdtempSync, openSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
  string,
} from "@kite/protocol";
import { sleep } from "@kite/runtime";
import { RpcError, RpcProcess, TurnCancelled } from "@kite/runtime/rpc";
import { nativeModelId } from "@kite/runtime/socket";
import { sourceLine } from "../history";
export class Ply implements NativeAdapter {
  models: Model[] = [];
  currentModel = "";
  currentEffort = "";
  readonly path = mkdtempSync(join(tmpdir(), "kite-ply-"));
  readonly record = join(this.path, "events.jsonl");
  readonly rpc: RpcProcess;
  private snapshot: Json | undefined;
  private cursor = 0;
  private calls = new Set<Json>();
  private requestId: number | undefined;
  private cancelled = false;
  constructor(private options: NativeOptions) {
    this.snapshot = options.saved;
    this.rpc = new RpcProcess([...options.command, "--record", this.record], options.cwd);
  }
  async initialize(): Promise<this> {
    try {
      const hello = object(
        await this.rpc.call(
          "initialize",
          {
            protocolVersion: "2025-06-18",
            clientInfo: { name: "kite", version: "2" },
            capabilities: {},
          },
          15000,
        ),
      );
      check(
        object(hello.serverInfo).name === "ply" &&
          object(object(hello.capabilities).experimental).plyAgent === 1,
        "Ply host chat capability unavailable",
      );
      this.rpc.write({ method: "notifications/initialized" });
      this.models = array(await this.rpc.call("ply/agent/catalog", {}, 15000)).map((value) => {
        const r = object(value);
        return {
          model: string(r.id),
          displayName: string(r.name),
          description: "",
          hidden: false,
          isDefault: nativeModelId(string(r.id)) === this.options.model,
          inputModalities: array(r.input).map(string),
          supportedReasoningEfforts: array(r.efforts).map((e) => ({
            reasoningEffort: string(e),
            description: string(e),
          })),
          defaultReasoningEffort: string(r.defaultEffort),
        };
      });
      const selected = this.models.find((r) => r.isDefault);
      check(selected && this.models.length <= 512, "Ply model catalog unavailable");
      this.currentModel = selected.model;
      this.currentEffort = selected.defaultReasoningEffort;
      return this;
    } catch (error) {
      this.close();
      throw error;
    }
  }
  alive(): boolean {
    return this.rpc.child.exitCode === null;
  }
  prepare(): void {
    this.cancelled = false;
    this.requestId = undefined;
  }
  async steer(): Promise<boolean> {
    return false;
  }
  async run(blocks: NativeBlock[], model: string, effort: string): Promise<string> {
    let stop = false;
    const seen = new Set<Json>();
    const watcher = (async () => {
      while (!stop) {
        await sleep(150);
        if (stop) break;
        try {
          this.events();
          const state = object(await this.rpc.call("ply/agent/state", {}));
          for (const value of array(state.pending ?? [])) {
            const r = object(value);
            const id = r.id;
            check(id !== undefined, "approval identity missing");
            if (!seen.has(id)) {
              seen.add(id);
              this.options.permission({
                id,
                params: {
                  toolCall: { toolCallId: id, title: r.command ?? r.tool, rawInput: r.input ?? {} },
                },
              });
            }
          }
        } catch {
          break;
        }
      }
    })();
    try {
      const value = object(
        await this.rpc.call(
          "ply/agent/chat",
          json({
            prompt: blocks
              .filter((b) => b.type === "text")
              .map((b) => b.text)
              .join("\n"),
            images: blocks.filter((b) => b.type === "image"),
            model,
            effort,
            snapshot: this.snapshot ?? null,
          }),
          3600000,
          (id) => {
            this.requestId = id;
            if (this.cancelled) this.cancel();
          },
        ),
      );
      const result = object(value.result);
      check(
        result.status !== "approval_required" && string(result.text).trim(),
        "Ply did not complete its turn",
      );
      check(value.snapshot !== undefined, "Ply snapshot missing");
      this.snapshot = value.snapshot;
      this.options.persist(this.snapshot);
      this.currentModel = model;
      this.currentEffort = effort;
      return string(result.text);
    } catch (error) {
      if (error instanceof RpcError && error.code === -32800) throw new TurnCancelled();
      throw error;
    } finally {
      stop = true;
      await watcher;
      this.events();
      this.requestId = undefined;
    }
  }
  private events(): void {
    if (!existsSync(this.record)) return;
    const fd = openSync(this.record, "r");
    try {
      while (true) {
        const row = sourceLine(fd, this.cursor, 40 * 1024 * 1024, true);
        if (!row.complete || row.text === null) return;
        this.cursor = row.end;
        const value = object(JSON.parse(row.text) as unknown);
        if (value.event === "spawn") {
          const f = object(value.fiber);
          if (
            ["bash", "read", "edit", "create", "remove", "skill", "agent", "memory"].includes(
              String(f.plugin),
            )
          ) {
            check(f.id !== undefined, "tool identity missing");
            this.calls.add(f.id);
            this.options.event({
              type: "tool/call",
              data: { callId: f.id, name: f.plugin, arguments: JSON.stringify(f.input ?? {}) },
            });
          }
        } else if (
          value.event === "exit" &&
          value.id !== undefined &&
          this.calls.delete(value.id)
        ) {
          const r = object(value.exit);
          this.options.event({
            type: "tool/result",
            data: {
              message: {
                toolCallId: value.id,
                content: [{ type: "text", text: JSON.stringify(r.output ?? { status: r.status }) }],
                isError: r.status !== "done",
              },
            },
          });
        }
      }
    } finally {
      closeSync(fd);
    }
  }
  cancel(): void {
    this.cancelled = true;
    if (this.requestId !== undefined)
      this.rpc.write({ method: "notifications/cancelled", params: { requestId: this.requestId } });
  }
  async approve(request: ObjectValue, accepted: boolean): Promise<void> {
    await this.rpc.call("ply/agent/approve", { id: request.id, accepted });
  }
  close(): void {
    this.rpc.close();
    rmSync(this.path, { recursive: true, force: true });
  }
}
