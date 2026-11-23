# RFC 0033：跨供应商长会话异步控制面与协作治理

> 编号：0033 · 状态：部分实现，分级验收见第 8 节 · 创建：2026-11-23
> 相关：[RFC 0017](0017-agent-notifications.md)、[RFC 0020](0020-session-owner-lifecycle.md)、[RFC 0026](0026-session-permissions.md)、[RFC 0027](0027-conversation-items-and-threads.md)、[RFC 0030](0030-network-acp-and-shared-workspaces.md)、[RFC 0032](0032-antigravity-acp.md)
> 配套实施规范：[跨供应商长会话协作开发规范](../docs/development/cross-provider-collaboration.md)

## 1. 背景与问题

[RFC 0030](0030-network-acp-and-shared-workspaces.md) 落地了 Kite 网络 ACP 首阶段（WebSocket `/acp`、`kite acp task`、`kite acp mcp` 的 `kite_remote_task`），已经能够支持任意供应商的调用方显式把单次短任务委派给本机或远端主机上已验证的 `codex` / `ply` / `dsh` / `antigravity` 会话。

但在**长会话、多轮深度协作与多供应商并行开发**场景下，现有首阶段实现暴露出 6 个结构性缺口：

1. **同步阻塞 + 10 分钟硬超时 + 断线盲飞**：`session/prompt` 同步占住单条 RPC 最多 10 分钟；一旦任务超时或 SSH 隧道抖动断开，远端 owner 继续运行但调用方既无法查询真实运行态，又因 `loadSession = false` 无法读取断线期间完成的最终输出与工具结果。再次发送 `session/prompt` 只会触发 `remote session already has an active turn`。
2. **调用侧缺少会话管理闭环**：`kite acp mcp` 仅暴露 `kite_remote_task` 一个工具，缺少 `list`、`status`、`read`、`wait_any`、`cancel`、`release`。主 Agent 在长会话经历上下文压缩（compaction）后容易丢失 `sessionId` 映射，任务结束后也无法归档并释放远端 owner，导致内存预算与侧栏持续泄漏。
3. **审批与提问模型不适配长程编码**：`acpTask` 仅对精确匹配 `allowCommands` 且 `raw.cwd === options.cwd` 的命令放行；任何参数变动、子目录命令或 `fileChange` 审批均被直接拒绝，非审批类提问直接返回 `-32601`，长程编码任务极易卡死或空转。
4. **缺少父子血缘（Lineage），人类侧栏难以管理**：通过 `kite acp` 派生的跨供应商会话在目标主机上是完全平级的普通顶层会话，无法区分来源主会话、子任务角色与阻塞状态，高频派发会迅速淹没侧栏。
5. **无法按子任务动态选择模型、思考强度与附件**：`KiteAcp` 当前直接继承主机默认模型与 `effort`，调用方不能按任务难度指定 `model` / `effort` / `serviceTier`，也无法传递目标机附件或截图引用。
6. **缺少并行聚合等待与工作区防踩踏约定**：并行派发多个供应商时只能轮询浪费 token；多个写入型子会话指向同一 `cwd` 时缺少活跃写会话冲突检测或 worktree 隔离契约。

## 2. 目标与非目标

### 目标

将 Kite 网络 ACP 升级为**异步、可观测、人类可随时介入的跨供应商会话控制面**：

1. **异步句柄 + 有界只读恢复**：提交与等待解耦；断线、超时或主 Agent 上下文压缩后，均可通过 `list` + `status` + `read` 无损恢复全局协作状态并取回最终输出与工具结果摘要。
2. **完整生命周期闭环**：调用方可在授权范围内创建、发送、观测、聚合等待、中断、归档并经既有 `HarnessLifecycle` 释放远端 owner。
3. **人机协同审批路由**：把远端长会话的审批与澄清问题从“静默拒绝”升级为显式策略，支持冒泡至 Kite 通知链路（[RFC 0017](0017-agent-notifications.md)）由人类在手机/桌面端一键批准或接管。
4. **父子血缘与关联任务归组**：持久化跨供应商/跨机器派生关系，Agent 可按父会话筛选恢复，人类可在 Kite 客户端以“关联任务”平层折叠方式清晰管理与随时接管。
5. **可控的资源与并发隔离**：支持按任务指定 `model` / `effort` / 权限策略 / 附件引用，并提供工作区写入冲突保护与 worktree 推荐契约。

### 不做的事（非目标）

- **不在 Kite 内部建设重型多 Agent 编排引擎或 DAG 调度器**：任务拆解、重试策略与多轮决策仍归主 Agent 或人类负责，Kite 只提供可靠的会话控制面与观测面。
- **不在网络层新增平行模型循环**：所有对话循环、凭据、工具执行与原始历史仍归执行主机上的唯一 provider owner。
- **不自动重放用户输入、审批或构建**：断线与超时只可幂等查询状态与读取结果，绝不隐式重发写操作。
- **不强求实现 ACP 全量 `loadSession` 历史重放**：长会话恢复通过有界最近轮次/摘要读取即可满足，完整历史浏览仍由 Kite 正常主机通道负责。
- **不在本 RFC 中改变 `claude` 的终端型集成架构**：`claude` 作为被调用端接入 `CHAT_PROVIDERS` 列为后续独立评估项（见待定问题）。

## 3. 架构与接口设计

严格遵循现有 `packages/*`（`@kite/protocol` → `@kite/runtime` → `@kite/platform` → `@kite/providers` → `@kite/host`）与 `crates/*`（`kite-agent` → `kite-app` → `kite-ffi` → 原生客户端）单向依赖边界。

### 3.1 会话血缘与控制面数据模型（`@kite/protocol` & `kite-agent`）

在主机会话目录记录 `SessionRecord` 中增加**向后兼容的可选字段**（旧记录缺失时按独立顶层会话处理）：

```ts
export interface SessionParent {
  machine: string;      // 发起方主机标识
  sessionId: string;    // 发起方 Kite 会话 ID（若由非 Kite 会话发起可用外部 opaque callerId）
  provider?: string;    // 发起方 provider 名称（如 "antigravity" | "codex" | "claude"）
}

export type DelegationPolicy = "strict" | "human_in_the_loop" | "isolated_auto";

export interface SessionDelegation {
  parent?: SessionParent;
  label?: string;       // 短任务标题（<= 120 字符），用于侧栏/关联任务展示
  role?: string;        // 角色标签（如 "reviewer" | "builder" | "researcher"）
  policy: DelegationPolicy;
  worktreeIsolated?: boolean; // 是否已由调用方隔离在独立 worktree/目录
  writeLock?: boolean;        // 是否声明写入当前 cwd（默认只读观测为 false，编码任务为 true）
}
```

运行态统一归约为 6 个显式状态（与 Rust 侧关联任务状态一致）：

```ts
export type RemoteSessionState =
  | "idle"
  | "working"
  | "waiting_approval"
  | "waiting_question"
  | "failed"
  | "released";
```

### 3.2 ACP v1 扩展协议（`@kite/providers/src/acp.ts`）

继续复用 WebSocket `/acp` 的既有认证、nonce/HMAC proof、loopback + SSH 隧道与工作区白名单；在 `initialize` 的 `_meta.kite` 中声明能力版本 `controlPlaneVersion: 2` 及支持的扩展方法：

| 方法 | 方向 | 语义与关键约束 |
| --- | --- | --- |
| `session/new` | Client → Server | 扩展 `_meta.kite` 接收可选 `provider`、`title`、`model`、`effort`、`serviceTier`、`delegation`（含 `parent`/`label`/`role`/`policy`/`writeLock`/`worktreeIsolated`）。当 `writeLock = true` 且同 `cwd` 已存在另一活跃写入会话且未显式允许共享时，返回冲突错误并给出占用会话摘要。 |
| `session/list` | Client → Server | 扩展过滤参数 `_meta.kite`: `{ parentSessionId?, parentMachine?, label?, states?, includeArchived? }`；返回项附带 `_meta.kite.{ provider, state, activeTurnId, parent, label, role, gitBranch, worktree }`。 |
| `_kite/session/submit` | Client → Server | **非阻塞提交一轮 prompt**。参数包含 `sessionId`、`prompt`（支持 `text` 与受 workspace 白名单校验的 `resource_link`）、可选 `expectedRevision`、`model`、`effort`、`serviceTier`、`policy`、`allowCommands`、`waitMs`（0..30000，默认 0 立即返回）。立即返回 `{ sessionId, turnId, state, completed?, reply?, stopReason? }`；若在 `waitMs` 内已完成则直接带回结果，否则返回 `state: "working"` 与稳定 `turnId`。 |
| `_kite/session/status` | Client → Server | **幂等只读查询**。返回真实 owner 状态（`RemoteSessionState`）、`activeTurnId`、`lastTurnId`、`lastTurnStatus`、`pendingInteraction`（若有挂起的审批命令或提问摘要，供调用方诊断）、`updatedAt`、`model`、`effort`、`cwd`。不触发任何模型 turn。 |
| `_kite/session/read` | Client → Server | **有界只读结果取回**。参数 `{ sessionId, turnId?, maxBytes? (<= 256 KiB), includeToolSummaries? }`。从既有历史/投影存储与活动 owner 状态中读取指定 `turnId`（默认最近一轮）的最终公开回复、关键工具调用摘要（命令/状态/截断输出）及 `stopReason`。`loadSession` 仍保持 `false`，不承诺全量历史重放。 |
| `_kite/session/wait_any` | Client → Server | **单 peer 多会话事件聚合等待**。参数 `{ sessionIds: string[], since?: Record<string, string>, timeoutMs: number (<= 60000) }`。在任意目标会话进入 `idle`（turn 完成）、`failed`、`waiting_approval` 或 `waiting_question` 时立即返回状态快照；超时返回 `{ timedOut: true, sessions }`，不报错、不取消远端任务。 |
| `session/cancel` | Client → Server | 沿用现有语义，补充允许基于 `sessionId` 中断即使不是当前连接创建的活跃 turn（经鉴权并核对会话归属同工作区）。 |
| `_kite/session/release` | Client → Server | 显式归档并调用既有 `HarnessLifecycle`（[RFC 0020](0020-session-owner-lifecycle.md)）释放空闲 owner 及其本地子进程；若当前仍有活跃 turn，支持 `forceCancel?: boolean`（默认 `false`，拒绝释放并返回 `working`）。保留原始 provider 历史与会话引用。 |

### 3.3 权限与人工介入策略（三档分流）

在 `KiteAcp` 收到后端 `requestApproval` 或提问交互时，按该会话/该轮生效的 `DelegationPolicy` 明确分流：

1. **`strict`（保守默认，兼容 RFC 0030）**：
   - 仅当命令精确命中 `allowCommands` 且 `cwd` 匹配时回 `accept`；其余审批直接 `decline`；非审批提问返回 `-32601`。适用于无人值守构建与确定性校验脚本。
2. **`human_in_the_loop`（长会话推荐默认）**：
   - 远端 owner 发出审批或提问请求时，**不自动拒绝**，而是保留在 owner 的待处理交互队列中，使会话状态进入 `waiting_approval` / `waiting_question`；
   - 通过主机通知日志（[RFC 0017](0017-agent-notifications.md)）触发事件推送，用户手机/桌面客户端收到带完整命令与工作目录的通知；
   - 人类在 Kite 客户端直接点“允许一次 / 拒绝”或提交问题回答后，远端 owner 继续执行；主 Agent 通过 `status` / `wait_any` 感知状态从 `waiting_*` 回到 `working` / `idle`。
   - 超过可配置挂起保护时长（默认沿用 owner 自身交互生命周期，客户端随时可 `cancel`）不会误杀后台上下文。
3. **`isolated_auto`（隔离自动模式）**：
   - 前置校验：`cwd` 必须显式声明 `worktreeIsolated = true` 且主机端验证其与已知其他活跃会话的工作目录不重合（或为独立 git worktree 路径）；
   - 仅对明确支持受控自动权限的 provider 透传对应会话级策略（例如 Antigravity `tool_permission = turbo`、Ply `approval_mode = bypass`、Codex 经 `configRequirements/read` 允许的 workspace 策略），**不得绕过主机管理约束或修改全局配置**。

### 3.4 `kite acp` CLI 与 `kite acp mcp` 管理面（`@kite/host/src/acp.ts`）

`kite acp mcp` 从单一工具升级为完整、精简的控制面工具集（同时保留 `kite_remote_task` 作为向后兼容的短任务快捷封装）：

| MCP 工具 / CLI 子命令 | 作用 | 关键参数 |
| --- | --- | --- |
| `kite_session_spawn` / `kite acp spawn` | 创建跨机/跨供应商会话并返回 `sessionId` | `peer`, `cwd`, `provider`, `label?`, `role?`, `model?`, `effort?`, `policy?`, `parent?`, `writeLock?`, `worktreeIsolated?` |
| `kite_session_send` / `kite acp send` | 向已有会话提交一轮任务（默认异步，可选短 `waitMs`） | `peer`, `sessionId`, `prompt`, `attachments?`, `expectedRevision?`, `model?`, `effort?`, `policy?`, `allowCommands?`, `waitMs?` |
| `kite_session_status` / `kite acp status` | 幂等查询状态、当前 turn 与挂起交互摘要 | `peer`, `sessionId` |
| `kite_session_read` / `kite acp read` | 有界读取指定/最近一轮最终文本与工具摘要 | `peer`, `sessionId`, `turnId?`, `maxBytes?` |
| `kite_session_list` / `kite acp list` | 按 peer / 父会话 / 状态列出受管会话 | `peer`, `parentSessionId?`, `states?`, `includeArchived?` |
| `kite_session_wait_any` / `kite acp wait` | 跨一个或多个 peer 聚合等待任意目标状态变化 | `targets: [{ peer, sessionId }]`, `timeoutMs` |
| `kite_session_cancel` / `kite acp cancel` | 中断目标会话当前活跃 turn | `peer`, `sessionId` |
| `kite_session_release` / `kite acp release` | 归档并释放远端 owner 进程资源 | `peer`, `sessionId`, `forceCancel?` |

多 peer `wait_any` 由调用侧（`@kite/host`）并行挂接各 peer 的单机 `_kite/session/wait_any`，首个产生状态跃迁时即刻返回并关闭其余等待请求，避免在网关间引入网状互连。

### 3.5 客户端“关联任务”统一收纳（Rust Core & 原生 UI）

- `kite-agent` 与 `kite-app` 将现有的 Codex 关联任务发现模型泛化为统一的 `AssociatedTask` 视图来源：
  1. 来源 A：Codex 原生 `thread/list` 后代（保持现有只读语义不变）；
  2. 来源 B：带有 `delegation.parent` 指向当前 `(host_id, session_id)` 的跨供应商/跨机器派生会话。
- 原生侧栏主列表默认折叠或弱化显示由另一活跃会话派生的子会话（用户仍可通过全局搜索或展开“关联任务”直接查看）；
- 在主会话的“关联任务”平层折叠区中展示子任务的标题（`label`）、目标主机、provider 标识与状态徽章（运行中 / 待审批 / 待回答 / 已完成 / 失败）；点击可只读预览最近回复，并提供显式“打开并接管”按钮原子跳转至目标机器与会话处理审批或人工介入。

### 3.6 安全、并发与资源上限

- 保持现有 loopback + SSH 信任转发、Bearer + HMAC proof 与工作区 `realpath` 白名单。
- 单 MCP 实例并发请求上限保持有界（`wait_any` 与只读查询和写提交分桶计数，防止长等待饿死 `cancel`/`status`）。
- 单父会话默认活跃未释放子会话配额上限为 `16`（超限时必须先 `release` 已完成子会话），防止失控循环派生耗尽主机 cgroup 预算。
- `read` 单次响应硬上限 `256 KiB`（远低于 `ACP_MESSAGE_LIMIT` 的 `2 MiB`），避免大输出撑爆调用方上下文窗口。

## 4. 备选方案

1. **在 Kite 内实现中心化 Swarm / 消息总线引擎**
   - 放弃原因：违背“Kite 不新增平行模型循环”的架构铁律；会把编排状态与 provider 上下文耦合在网关进程里，重启即丢失，且难以兼容各供应商不断演进的原生工具与权限模型。
2. **实现完整 ACP `loadSession = true` 全量历史重放**
   - 放弃原因：长会话历史可达数 MB，每次断线重连全量重放会瞬间撑爆调用方 Agent 上下文窗口，且跨 provider 历史格式差异大。只读按轮次有界 `read` 即可覆盖 99% 的长会话协作恢复需求。
3. **跨供应商任务默认全自动放行工具审批**
   - 放弃原因：风险不可控，且违背 [RFC 0026](0026-session-permissions.md)“显式配置、不绕过管理约束”的原则。通过 `human_in_the_loop` 接入 Kite 既有通知与审批卡，兼顾安全性与长程可用性。

## 5. 分阶段实施路线

- **阶段 P0（服务端异步控制面）**：`@kite/protocol` + `@kite/providers/src/acp.ts` + `@kite/host/src/acp.ts` 实现 `spawn / send / status / read / list / wait_any / cancel / release`、模型/思考强度参数透传、有界读回与单测/夹具回归。
- **阶段 P1（血缘持久化 + 人工审批冒泡 + 客户端收纳）**：主机目录持久化 `delegation` 元数据、`human_in_the_loop` 审批对接 RFC 0017 通知、Rust Core 泛化 `AssociatedTask`、Android/macOS/Windows 侧栏与关联任务展示及跳转接管。
- **阶段 P2（并发防踩踏与全供应商对称性）**：工作区写锁与 worktree 校验、`isolated_auto` 隔离模式、跨机附件引用传递，以及独立评估 `claude` 接入 `CHAT_PROVIDERS` 的可行性。

## 6. 对 SPEC 的影响

本 RFC 采纳并进入实现后，将在 [docs/spec/network-acp-and-collaboration.md](../docs/spec/network-acp-and-collaboration.md) 中把第 2 节由“草案”转为“生效”，并同步在 [docs/spec/sessions-and-providers.md](../docs/spec/sessions-and-providers.md) 中把“关联任务”规则从 Codex 专属扩展为“Codex 原生后代 + 跨供应商派生会话统一视图”。

## 7. 待定问题

1. **`kite_remote_task` 的去留**：在阶段 P0 完成后，保留 `kite_remote_task` 作为内部调用 `spawn/send(waitMs)` 的便捷单次工具，还是在过渡期后移除？倾向于保留一个短平快同步封装，避免破坏既有自动化脚本。
2. **`claude` 作为被调用端的接入路径**：Claude Code 后续是否通过 `claude -p --output-format stream-json` 或官方 ACP 桥接纳入 `CHAT_PROVIDERS`？需要单独评估其长会话恢复与审批协议后另立 RFC 或补充至 [harness 接入规范](../docs/development/harness-integration.md)。
3. **跨机器 `parent` 反向索引的离线表现**：当子会话所在远端机器离线时，主会话所在客户端的“关联任务”区如何展示离线缓存状态（沿用现有多主机离线目录缓存，不把缺失实时事实显示为“运行中”）。

## 8. 实施与验收记录

- **静态与离线**。Bun 类型、lint、单向依赖与测试，Rust workspace 测试均通过。覆盖模型、思考强度、服务档位校验，UTF-8 和 JSON 转义下的读回上限，血缘 CAS 和旧记录兼容，Rust 关联任务合并、离线状态与通知去重。Windows C ABI 特性测试通过，Kotlin 与 Swift 绑定使用同版编译库生成并核对字段。
- **协议夹具**。在独立临时目录和 loopback 端口运行打包的 `kite acp serve`。真实 WebSocket 与本机 Unix Socket 连接到确定性的 Ply 协议夹具，验证断开后观察待审批状态、原生协议响应后读回同一轮回复和工具退出码、仅执行一次、父会话配额、同 cwd 冲突、原始未知 provider 行的无损写回。移除夹具中的近期投影后从原始轮次历史读回。另有真实 stdio MCP 与双 peer WebSocket 验证聚合等待、取消和状态请求分桶，以及关闭等待连接。
- **隔离策略**。本地 Git linked worktree 夹具覆盖两个不同 provider 并发、同目录冲突、越界附件拒绝、不支持自动档位时拒绝，以及复用 fixture 已启用的 Ply 自动档位。空闲及强制取消后释放由协议夹具和现有生命周期回归覆盖。
- **独立网关**。安装器检测活跃的 `kite-acp.service` 并报告该单元独立管理。兼容型 Node 热交接由 `kite-node upgrade` 管理自己的监听器和代次，另一个单元中已加载的适配器由该服务的部署流程刷新。
- **待验收**。真实跨机与真实模型联调、真实系统通知至手机或桌面、原生设备交互、超过旧十分钟窗口的持续任务和正式发布包属于后续证据。
