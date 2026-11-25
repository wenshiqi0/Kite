# RFC 0035：受管模型服务的全局系统提示词与会话级 Kite MCP 注入

> 编号：0035 · 状态：已实现 · 创建：2026-11-25
> 相关：[RFC 0019](0019-agent-docs-and-linux-desktop.md)、[RFC 0023](0023-harness-capability-parity.md)、[RFC 0032](0032-antigravity-acp.md)、[RFC 0033](0033-cross-provider-async-session-control.md)

## 背景

Kite 此前仅通过环境变量 `KITE_SESSION_ID` 关联受管会话与父子血缘，未在启动模型服务或建立会话时注入统一的 Kite 系统提示词和会话级 Kite MCP 服务（ACP 适配器中 `mcpServers` 均为空数组）。结果导致：

- 模型服务（例如 Antigravity）在未手工配置 harness 全局规则与 `kite acp mcp` 时，会把用户的“子任务”误解为 harness 内置孤立 subagent，而不是走 `kite acp` v2；
- Codex 依赖用户在 `~/.codex/AGENTS.md` 手工维护派发规则，不同 provider 行为分裂。

## 目标

- 在 `@kite/providers` 中以单一真源定义 Kite 托管会话系统提示词（`KITE_SYSTEM_PROMPT`），明确：
  1. 子任务与跨供应商委派优先使用 Kite 会话控制 v2（`kite acp` / `kite_session_*`），不误用 harness 本地孤立 subagent；
  2. 宿主机查询与 Linux X11 桌面操作走 Kite（`kite docs` / `kite desktop` / `kite_*`）；
  3. 需要用户澄清或选择时优先使用 provider 提供的结构化提问工具。
- 启动或恢复受管会话时，通过各 provider 原生非破坏通道注入系统提示词与会话级 `kite` / `kite_acp` MCP 服务：
  - **Codex**：`codex app-server` 启动参数 `-c developer_instructions=...` 与 `-c mcp_servers.kite.*` / `mcp_servers.kite_acp.*`（TOML 嵌套合并，不改 `~/.codex/config.toml` 与 `AGENTS.md`），macOS `thread/start` 同步携带 `developerInstructions`；
  - **Antigravity**：当 `initialize` 广播 `_meta.antigravity.systemPrompt.append === true` 时，在 `session/new` / `session/resume` / `session/load` 携带 `_meta.antigravity.systemPrompt.append` 与 `mcpServers`（显式透传 `KITE_SESSION_ID`）；
  - **Ply / DSH**：在 `session/new` / `session/load` / `session/resume` 注入会话级 `mcpServers`，并在 provider 广播 `_meta.systemPrompt` 能力时追加系统提示词。

**不做的事：**

- 不修改用户全局配置文件（`~/.codex/config.toml`、`~/.gemini/config/mcp_config.json`、`AGENTS.md`）；
- 不使用 `override` 覆盖用户或项目规则，只做追加（`append` / `developer_instructions`）；
- 只读发现探针（`inspect: true`）不启动会话，也不预加载 MCP 服务；
- 不把系统提示词塞入用户可见消息或聊天历史。

## 设计

1. 新增 [kite-context.ts](../packages/providers/src/kite-context.ts)，统一导出：
   - `KITE_SYSTEM_PROMPT`
   - `kiteCliPath()`（依次查找 `KITE_CLI_PATH`、`~/.local/bin/kite`、`PATH` 中的 `kite`）
   - `kiteMcpServers()`（输出 `kite` 与 `kite_acp` 两个 ACP stdio MCP 条目，显式转发 `KITE_SESSION_ID`）
   - `antigravitySystemPromptAppendSupported()` / `antigravitySessionMeta()`
   - `acpSystemPromptMeta()`
   - `codexConfigOverrides()`
2. 四个 provider 适配器（`codex.ts`、`native/antigravity.ts`、`native/ply.ts`、`native/dsh.ts`）统一消费上述入口。
3. 同步更新 [skills/kite/SKILL.md](../skills/kite/SKILL.md) 描述，覆盖子任务协作入口（`kite docs collaboration`）。

## 备选方案

- **仅在用户各 harness 全局规则中手工补写**：无法保证多机、多 provider 一致性，且切换或新装 provider 仍会遗漏，故否决。
- **在首条用户消息前拼接文本块**：会污染原生聊天历史与 UI 展示，故优先使用各 provider 原生系统指令与 MCP 通道。

## 对 SPEC 的影响

- 在 [docs/spec/sessions-and-providers.md](../docs/spec/sessions-and-providers.md) 的“统一目录与能力对齐”与各 provider 规则中补充托管会话全局系统提示词与会话级 Kite MCP 注入约束。

## 待定问题

- Ply 与 DSH 服务端如需支持 `_meta.systemPrompt` 追加，可按标准 ACP 能力开关对接，无需再改 Kite 协议。
