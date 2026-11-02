/** Runs inside DSH's native Node process; only public metadata crosses this boundary. */
import { chmodSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createServer } from "node:net";
import { pathToFileURL } from "node:url";

interface Reasoning {
  defaultEffort?: string;
  efforts: { id: string; name: string; description?: string }[];
}
interface Context {
  llm: {
    listProviders(): { id: string }[];
    listModels(id: string): Promise<{ id: string }[]>;
    resolveModelInfo(
      provider: string,
      id: string,
    ): Promise<{
      name?: string;
      description?: string;
      inputModalities?: string[];
      reasoning?: Reasoning;
    }>;
  };
  agents: Map<string, { status: string; session: unknown; steer(message: unknown): void }>;
  sessions: { flush(session: unknown): Promise<void> };
  get(name: string): { await(): Promise<void> } | undefined;
  effect(fn: () => undefined | (() => void)): void;
}
interface Config {
  provider: string;
  output: string;
  control?: string;
}
export const inject = ["llm", "agents", "sessions"];
function startControl(ctx: Context, path: string): () => void {
  const server = createServer((socket) => {
    let buffer = "";
    socket.on("data", (data) => {
      buffer += data.toString();
      if (Buffer.byteLength(buffer) > 2 * 1024 * 1024) {
        socket.destroy();
        return;
      }
      if (!buffer.endsWith("\n")) return;
      socket.pause();
      void (async () => {
        try {
          const value: unknown = JSON.parse(buffer);
          if (
            !value ||
            typeof value !== "object" ||
            !("method" in value) ||
            value.method !== "steer" ||
            !("sessionId" in value) ||
            typeof value.sessionId !== "string" ||
            !("text" in value) ||
            typeof value.text !== "string" ||
            !value.text.trim()
          )
            throw new Error("invalid steering");
          const agent = ctx.agents.get(value.sessionId);
          if (!agent) throw new Error("session unavailable");
          if (agent.status === "idle") {
            socket.end(`${JSON.stringify({ accepted: false })}\n`);
            return;
          }
          const require = createRequire(process.argv[1] || import.meta.url);
          const mod: unknown = await import(
            pathToFileURL(require.resolve("@deepseek-ai/dsh-llm")).href
          );
          if (
            !mod ||
            typeof mod !== "object" ||
            !("createUserMessage" in mod) ||
            typeof mod.createUserMessage !== "function"
          )
            throw new Error("native message factory unavailable");
          if (agent.status === "idle") {
            socket.end(`${JSON.stringify({ accepted: false })}\n`);
            return;
          }
          const message: unknown = mod.createUserMessage({
            content: [{ type: "text", text: value.text }],
            source: { kind: "user" },
          });
          agent.steer(message);
          await ctx.sessions.flush(agent.session);
          socket.end(`${JSON.stringify({ accepted: true })}\n`);
        } catch {
          socket.end(`${JSON.stringify({ error: "steering_unavailable" })}\n`);
        }
      })();
    });
    socket.on("error", () => {});
  });
  server.on("error", () => {});
  server.listen(path, () => chmodSync(path, 0o600));
  return () => {
    server.close();
  };
}
export function apply(ctx: Context, config: Config): void {
  ctx.effect(() => {
    let stopped = false;
    setImmediate(() => {
      void (async () => {
        try {
          await ctx.get("loader")?.await();
          if (config.control) ctx.effect(() => startControl(ctx, config.control || ""));
          const models = [];
          for (const provider of ctx.llm.listProviders()) {
            if (provider.id !== config.provider) continue;
            for (const entry of await ctx.llm.listModels(provider.id)) {
              const info = await ctx.llm.resolveModelInfo(provider.id, entry.id);
              models.push({
                model: JSON.stringify([provider.id, entry.id]),
                displayName: info.name ?? entry.id,
                description: info.description ?? "",
                hidden: false,
                isDefault: false,
                inputModalities: info.inputModalities ?? ["text"],
                defaultReasoningEffort: info.reasoning?.defaultEffort ?? "",
                supportedReasoningEfforts: [
                  ...(info.reasoning && !info.reasoning.defaultEffort
                    ? [{ reasoningEffort: "", description: "Provider default" }]
                    : []),
                  ...(info.reasoning?.efforts ?? []).map((e) => ({
                    reasoningEffort: e.id,
                    description: e.description ?? e.name,
                  })),
                ],
              });
            }
          }
          if (!stopped) writeFileSync(config.output, JSON.stringify({ models }), { mode: 0o600 });
        } catch {
          if (!stopped)
            writeFileSync(config.output, JSON.stringify({ error: "catalog_unavailable" }), {
              mode: 0o600,
            });
        }
      })();
    });
    return () => {
      stopped = true;
    };
  });
}
