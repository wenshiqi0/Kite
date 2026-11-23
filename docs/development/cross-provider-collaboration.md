# 跨供应商长会话协作开发规范

> 范围：内部开发；在实施、修改或排查 [RFC 0033](../../rfcs/0033-cross-provider-async-session-control.md)（跨供应商长会话异步控制面）时阅读。
> 状态：开发规范（与 RFC 0033 草案配套；以 [SPEC](../../SPEC.md) 和 [网络 ACP 规范](../spec/network-acp-and-collaboration.md) 为边界）。

本文面向具体编码与验收环节，明确跨供应商长会话控制面的**代码落点、协议校验红线、状态机约束、安全边界与分阶段测试验收标准**。

---

## 1. 模块边界与代码落点

任何实现必须通过 `bun run check`（含 `scripts/check-boundaries.ts`）与 `cargo test`，严格遵守既有单向依赖：

| 层次 / 文件 | 负责内容 | 禁止事项 |
| --- | --- | --- |
| `packages/protocol/src/index.ts`<br>`packages/protocol/src/acp.ts` | 新增 `SessionParent`、`SessionDelegation`、`DelegationPolicy`、`RemoteSessionState` schema 校验；定义 `ACP_READ_LIMIT`（`256 KiB`）、`ACP_MAX_CHILDREN_PER_PARENT`（`16`）等常量 | 不得引入 Node/Bun 进程或文件系统依赖；不得使用 `any` 或跳过字段校验 |
| `packages/providers/src/catalog.ts` | 在 `SessionRecord` 读写与原子修订号 CAS 写入中无损保留可选 `delegation` 字段 | 不得破坏旧版 `sessions.json` 读取；不得在目录中存对话正文或密钥 |
| `packages/providers/src/acp.ts` | 升级 `KiteAcp`：`initialize` 能力声明（`controlPlaneVersion: 2`）；支持 `model`/`effort`/`serviceTier` 透传；实现 `_kite/session/submit`、`status`、`read`、`wait_any`、`release`；实现三档 `DelegationPolicy` 审批分流与同 `cwd` 写锁冲突检测 | **严禁**在 `KiteAcp` 中新增独立模型循环；**严禁**绕过 `HarnessLifecycle` 直接 `kill` 未知进程；**严禁**在断线时重放 prompt 或审批 |
| `packages/host/src/acp.ts` | 扩展 `kite acp` CLI 子命令与 `kite acp mcp` 完整工具集（`spawn` / `send` / `status` / `read` / `list` / `wait_any` / `cancel` / `release` + 兼容 `kite_remote_task`）；实现跨 peer 并行 `wait_any` 与读写分桶并发限制 | 不得把 token、SSH 私钥或 provider API key 打印到 stdout/stderr/日志；不得取消 `StrictHostKeyChecking=yes` |
| `crates/kite-agent/src/sessions.rs`<br>`crates/kite-agent/src/subagents.rs` | 解析并持久化 `SessionDelegation`；将 Codex 原生后代与跨供应商派生会话统一投影为 `AssociatedTask` 结构化模型 | 不得在 Rust 中拼接 UI 文案；不得把缺失实时事实伪造成 `working` |
| `crates/kite-app/src/core.rs`<br>`crates/kite-app/src/host_node/` | 多主机目录下关联父子会话、推送待审批/待回答通知（复用 RFC 0017）、处理客户端“打开并接管”原子切主机切会话 | 不得在点击通知时自动批准或发送消息；不得在切换选中项时丢失后台运行态 |
| `crates/kite-ffi/` 与原生客户端（`android/`、`macos/`、`windows/`） | 暴露更新后的 `AssociatedTask`（含来源主机、provider、状态与打开接管动作）；与主聊天复用同一条目渲染组件 | 不得在 ViewModel 实现协议逻辑；不得手改 UniFFI 生成绑定 |
| `docs/agent/collaboration.md` | 仅在能力真实落地验收后，同步更新暴露给运行中 Agent 的精简使用指南 | 未实现的能力不得提前写入 `docs/agent/` |

---

## 2. 状态机与幂等红线

### 2.1 状态推导规则

服务端 `_kite/session/status` 必须按真实 owner 运行事实推导 `RemoteSessionState`，优先级如下：

1. 已归档且无运行 owner → `released`
2. owner 存在未决审批请求 → `waiting_approval`
3. owner 存在未决提问请求 → `waiting_question`
4. owner 存在进行中 turn → `working`
5. 最近一轮 turn 终态为 `failed` 或 owner 异常中断 → `failed`
6. 其余空闲态 → `idle`

禁止凭“WebSocket 刚发完请求”或过期缓存猜测状态；无法核实 owner 状态时必须返回明确错误，不得静默伪装为 `idle`。

### 2.2 不重放与幂等边界

| 操作 | 是否幂等 | 断线 / 超时处理规则 |
| --- | --- | --- |
| `list` / `status` / `read` / `wait_any` | **是** | 允许调用方安全重试 |
| `spawn` | 否（新建会话） | 断线后调用方须先 `list(parentSessionId, label)` 检查是否已创建，避免重复派生 |
| `send` / `_kite/session/submit` | **否** | **绝不自动重放**；调用方须先 `status` 检查 `activeTurnId` / `lastTurnId` 再决定是否 `read` 或补发 |
| `cancel` | 准幂等 | 若目标已空闲返回当前终态而非破坏状态 |
| `release` | 准幂等 | 校验活跃 turn（默认拒绝释放运行中任务）；复用 `HarnessLifecycle` 确保上下文已落盘后再回收 |

---

## 3. 安全与资源防护要求

1. **工作区与附件路径边界**：
   - `cwd` 与所有本地附件 `resource_link` 路径都必须经 `realpathSync` 校验位于 `--workspace` 配置根目录内，拒绝符号链接逃逸与 `..` 穿越。
2. **三档权限策略校验**：
   - `strict`：严格沿用 `allowCommands` 精确字符串匹配且 `raw.cwd === session.cwd`。
   - `human_in_the_loop`：仅当主机启用通知链路或本地人类客户端可接管时生效；挂起请求保留原始 `nativeId` 与完整命令参数供客户端审批卡展示，不得自动降级为 `accept`。
   - `isolated_auto`：必须同时满足“显式声明 `worktreeIsolated: true` + 服务端校验无其他活跃会话共享该 `cwd` + 目标 provider 自身支持并允许该权限档位”，任一条件不满足必须拒绝 `spawn`/`send`，不得静默回退到无限制权限。
3. **写锁与并发防踩踏**：
   - 同一 `cwd` 上存在 `writeLock: true` 且状态为 `working` / `waiting_approval` / `waiting_question` 的会话时，拒绝新的 `writeLock: true` 会话（除非调用方显式指定不同 worktree 路径）。
4. **配额与防爆上下文**：
   - 每个 `parentSessionId` 下活跃未释放子会话数上限 `16`；
   - `_kite/session/read` 默认截断上限 `64 KiB`，显式 `maxBytes` 最大 `256 KiB`，超长输出按尾部优先保留并显式标注 `truncated: true`；
   - `kite acp mcp` 读写分桶：写/长等操作与只读 `status`/`read`/`list`/`cancel` 互不阻塞。

---

## 4. 分阶段测试与验收矩阵

遵循 [SPEC 治理与验收基线](../spec/governance.md)，四类证据必须分别通过并记录，不得互相替代：

### 阶段 P0：异步控制面与协议层

- **静态与边界检查**：`bun run typecheck && bun run lint`（含 `scripts/check-boundaries.ts`）。
- **Bun 单测与真实 Socket 夹具**（`packages/providers/test/acp.test.ts`、`packages/host/test/acp.test.ts`）：
  - `submit` 非阻塞立即返回 `turnId`，长任务超过原超时窗口不断线丢失；
  - 模拟传输中途断开后重连，通过 `status` + `read` 准确取回断线后完成的最终回复与工具摘要，且确认远端 turn 仅执行 1 次（无重放）；
  - `wait_any` 多会话并发中，首个完成/进入等待审批立即返回，超时返回 `timedOut: true` 不取消远端任务；
  - `model` / `effort` 非法值拒绝、有效值准确送达后端；
  - `release` 拒绝释放运行中会话（除非 `forceCancel: true`），空闲会话确认落盘后释放 owner 且保留历史；
  - 超过每父会话 16 个子会话配额、越界路径、`read` 超字节上限均被正确拦截或截断。

### 阶段 P1：血缘、人工审批冒泡与客户端收纳

- **Rust 单测**（`cargo test`）：`SessionDelegation` 序列化兼容旧目录、`AssociatedTask` 合并 Codex 原生后代与跨供应商派生会话、离线主机不误标为运行中、点击通知仅导航不自动审批。
- **真实跨供应商联调验收**（本机 `wen-h255` 与/或 `wen-mini`，真实启用两个不同 provider，如 Antigravity ↔ Codex 或 Codex ↔ Ply）：
  - 主会话派生子会话执行真实代码/测试任务，途中主会话模拟断线/重置上下文后，通过 `list` + `status` + `read` 恢复并验收真实工具退出码；
  - 触发一次 `human_in_the_loop` 审批，验证通知送达客户端、人类批准后远端继续完成、主 Agent 通过 `wait_any` 感知完成；
  - 结束后调用 `release`，确认目标主机子会话 owner 进程退出、内存回落且会话历史仍可在 Kite 查看。

### 阶段 P2：隔离协作与对称扩展

- **Worktree 隔离验证**：两个不同供应商并发在同仓库不同 worktree 写入无冲突；指向同一 `cwd` 声明 `writeLock: true` 时被明确拦截。
- **权限边界验证**：验证 `isolated_auto` 在未隔离目录或不受管理约束允许时被拒绝，绝不修改全局 provider 配置。
