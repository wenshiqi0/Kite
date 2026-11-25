import { existsSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import type { ObjectValue } from "@kite/protocol";
import { home } from "@kite/runtime";
import { optionalObject } from "@kite/runtime/socket";

/**
 * Shared global system instructions injected by Kite into every managed model
 * service so Kite-owned capabilities stay consistent across all providers
 * without modifying user config files or overwriting user/project rules.
 */
export const KITE_SYSTEM_PROMPT = [
  "You are running inside a Kite-managed session on this host.",
  "- Subtasks and delegation: When the user asks to spawn, delegate, or run a subtask or child session, always use Kite session control v2 (`kite acp spawn/send/status/wait/read/release` or the `kite_session_*` MCP tools) rather than harness-local isolated subagents. Kite binds the active parent session automatically via `KITE_SESSION_ID` / `CODEX_THREAD_ID`.",
  "- Host and desktop operations: Use Kite (`kite docs`, `kite status`, `kite sessions`, `kite desktop`, or the `kite_*` MCP tools) for host queries and configured Linux X11 desktop control.",
  "- Clarifying questions: When asking the user to choose between options or clarify ambiguous requirements before acting, prefer the provider's structured user-question tool over plain-text option lists when available.",
  "- Read `kite docs collaboration` or `kite docs desktop` on demand when detailed Kite CLI/MCP usage is needed.",
].join("\n");

export interface AcpMcpServer {
  name: string;
  command: string;
  args: string[];
  env: { name: string; value: string }[];
}

export function kiteCliPath(
  env: NodeJS.ProcessEnv = process.env,
  which: (command: string) => string | null = (command) =>
    Bun.which(command, { PATH: env.PATH ?? "" }),
  exists: (path: string) => boolean = existsSync,
  homeDir: string = home(),
): string | null {
  const override = env.KITE_CLI_PATH?.trim();
  if (override) return isAbsolute(override) && exists(override) ? override : null;
  const local = join(homeDir, ".local/bin/kite");
  if (exists(local)) return local;
  return which("kite");
}

/**
 * Session-scoped Kite MCP servers injected into ACP sessions and Codex
 * app-server launches. Forwards `KITE_SESSION_ID` explicitly so child MCP
 * servers retain parent-session binding even when a provider filters env vars.
 */
export function kiteMcpServers(
  env: NodeJS.ProcessEnv = process.env,
  which?: (command: string) => string | null,
  exists?: (path: string) => boolean,
  homeDir?: string,
): AcpMcpServer[] {
  if (env.KITE_DISABLE_MCP_INJECTION === "1") return [];
  const command = kiteCliPath(env, which, exists, homeDir);
  if (!command) return [];
  const forwarded = env.KITE_SESSION_ID
    ? [{ name: "KITE_SESSION_ID", value: env.KITE_SESSION_ID }]
    : [];
  return [
    { name: "kite", command, args: ["mcp"], env: forwarded },
    { name: "kite_acp", command, args: ["acp", "mcp"], env: forwarded },
  ];
}

export function antigravitySystemPromptAppendSupported(capabilities: ObjectValue): boolean {
  const ag = optionalObject(optionalObject(capabilities._meta).antigravity);
  return optionalObject(ag.systemPrompt).append === true;
}

export function antigravitySessionMeta(supportsAppend: boolean): ObjectValue | undefined {
  if (!supportsAppend) return undefined;
  return {
    antigravity: {
      systemPrompt: {
        append: KITE_SYSTEM_PROMPT,
      },
    },
  };
}

export function acpSystemPromptMeta(capabilities: ObjectValue): ObjectValue | undefined {
  const meta = optionalObject(capabilities._meta);
  const sp = optionalObject(meta.systemPrompt);
  if (sp.append !== true && meta.systemPrompt !== true) return undefined;
  return {
    systemPrompt: {
      mode: "append",
      content: [{ type: "text", text: KITE_SYSTEM_PROMPT }],
    },
  };
}

/**
 * Codex `-c key=value` overrides injected into Kite-owned `codex app-server`
 * processes. Uses TOML basic strings so nested `[mcp_servers.*]` tables merge
 * with existing user config without modifying `~/.codex/config.toml`.
 */
export function codexConfigOverrides(
  env: NodeJS.ProcessEnv = process.env,
  which?: (command: string) => string | null,
  exists?: (path: string) => boolean,
  homeDir?: string,
): string[] {
  const args = ["-c", `developer_instructions=${JSON.stringify(KITE_SYSTEM_PROMPT)}`];
  for (const server of kiteMcpServers(env, which, exists, homeDir)) {
    args.push("-c", `mcp_servers.${server.name}.command=${JSON.stringify(server.command)}`);
    args.push("-c", `mcp_servers.${server.name}.args=${JSON.stringify(server.args)}`);
  }
  return args;
}
