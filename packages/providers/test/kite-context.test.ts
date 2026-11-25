import { expect, test } from "bun:test";
import {
  acpSystemPromptMeta,
  antigravitySessionMeta,
  antigravitySystemPromptAppendSupported,
  codexConfigOverrides,
  KITE_SYSTEM_PROMPT,
  kiteCliPath,
  kiteMcpServers,
} from "../src/kite-context";

test("KITE_SYSTEM_PROMPT covers subtasks, host/desktop, and structured questions", () => {
  expect(KITE_SYSTEM_PROMPT).toContain("kite acp");
  expect(KITE_SYSTEM_PROMPT).toContain("kite_session_*");
  expect(KITE_SYSTEM_PROMPT).toContain("kite desktop");
  expect(KITE_SYSTEM_PROMPT).toContain("structured user-question tool");
});

test("kiteCliPath prefers KITE_CLI_PATH, then ~/.local/bin/kite, then PATH", () => {
  const existing = new Set(["/custom/kite", "/home/u/.local/bin/kite"]);
  expect(
    kiteCliPath(
      { KITE_CLI_PATH: "/custom/kite" },
      () => "/usr/bin/kite",
      (p) => existing.has(p),
      "/home/u",
    ),
  ).toBe("/custom/kite");
  expect(
    kiteCliPath(
      {},
      () => "/usr/bin/kite",
      (p) => existing.has(p),
      "/home/u",
    ),
  ).toBe("/home/u/.local/bin/kite");
  expect(
    kiteCliPath(
      {},
      () => "/usr/bin/kite",
      () => false,
      "/home/u",
    ),
  ).toBe("/usr/bin/kite");
  expect(
    kiteCliPath(
      { KITE_CLI_PATH: "relative/kite" },
      () => "/usr/bin/kite",
      () => true,
      "/home/u",
    ),
  ).toBeNull();
});

test("kiteMcpServers injects kite and kite_acp and forwards KITE_SESSION_ID", () => {
  const servers = kiteMcpServers(
    { KITE_SESSION_ID: "a".repeat(32) },
    () => "/usr/local/bin/kite",
    () => false,
    "/home/u",
  );
  expect(servers).toEqual([
    {
      name: "kite",
      command: "/usr/local/bin/kite",
      args: ["mcp"],
      env: [{ name: "KITE_SESSION_ID", value: "a".repeat(32) }],
    },
    {
      name: "kite_acp",
      command: "/usr/local/bin/kite",
      args: ["acp", "mcp"],
      env: [{ name: "KITE_SESSION_ID", value: "a".repeat(32) }],
    },
  ]);

  expect(
    kiteMcpServers(
      { KITE_DISABLE_MCP_INJECTION: "1" },
      () => "/usr/local/bin/kite",
      () => true,
      "/home/u",
    ),
  ).toEqual([]);

  expect(
    kiteMcpServers(
      {},
      () => null,
      () => false,
      "/home/u",
    ),
  ).toEqual([]);
});

test("system prompt metadata is capability-gated for Antigravity and generic ACP", () => {
  expect(antigravitySystemPromptAppendSupported({})).toBe(false);
  expect(
    antigravitySystemPromptAppendSupported({
      _meta: { antigravity: { systemPrompt: { append: true } } },
    }),
  ).toBe(true);
  expect(antigravitySessionMeta(false)).toBeUndefined();
  expect(antigravitySessionMeta(true)).toEqual({
    antigravity: { systemPrompt: { append: KITE_SYSTEM_PROMPT } },
  });

  expect(acpSystemPromptMeta({})).toBeUndefined();
  expect(acpSystemPromptMeta({ _meta: { systemPrompt: { append: true } } })).toEqual({
    systemPrompt: {
      mode: "append",
      content: [{ type: "text", text: KITE_SYSTEM_PROMPT }],
    },
  });
});

test("codexConfigOverrides emits developer_instructions and Kite MCP overrides", () => {
  const args = codexConfigOverrides(
    {},
    () => "/home/u/.local/bin/kite",
    () => false,
    "/home/u",
  );
  expect(args).toEqual([
    "-c",
    `developer_instructions=${JSON.stringify(KITE_SYSTEM_PROMPT)}`,
    "-c",
    'mcp_servers.kite.command="/home/u/.local/bin/kite"',
    "-c",
    'mcp_servers.kite.args=["mcp"]',
    "-c",
    'mcp_servers.kite_acp.command="/home/u/.local/bin/kite"',
    "-c",
    'mcp_servers.kite_acp.args=["acp","mcp"]',
  ]);
});
