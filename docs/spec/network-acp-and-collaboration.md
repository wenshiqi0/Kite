# 网络 ACP 与跨供应商协作规范

> 所属规范：[SPEC.md](../../SPEC.md) · 状态：第 1 节记录 RFC 0030 及侧栏规则，第 2 节记录 RFC 0033 已通过代码与隔离夹具验证的规则 · 更新：2026-11-24
> 相关提案：[RFC 0030](../../rfcs/0030-network-acp-and-shared-workspaces.md)、[RFC 0033](../../rfcs/0033-cross-provider-async-session-control.md)

## 1. 已生效规则：多机器工作区与网络 ACP 首阶段（RFC 0030）

### 1.1 跨主机会话侧栏

- Rust Core 统一发布已保存主机的会话目录、能力、连接与选中状态，包含离线缓存，以及按工作区排序的 `(host_id, session_id)` 列表。
- 原生侧栏按统一顺序展示跨主机会话，提供搜索、新建和归档入口。会话项附带执行主机与 provider。主机管理从独立入口访问。
- 可见父会话下展示同机与跨主机关联任务。搜索可匹配子任务与其执行主机，接管时打开子会话的真实执行主机。
- 打开会话必须原子绑定目标主机与目标会话。新建会话必须显式选择执行主机与该主机探测可用的 provider。

### 1.2 网络 ACP 网关边界（WebSocket `/acp` v1）

- 网络入口采用 ACP v1 JSON-RPC over WebSocket `/acp`（对应官方网络传输 RFD 的 WebSocket profile，**不宣称**已实现完整 HTTP/2 + SSE profile）。
- **无平行模型循环**：Kite 网络 ACP 仅作为现有主机唯一 owner 的客户端代理，绝不在网络层新增独立模型循环。
- **支持范围**：
  - 协议操作：`initialize`、`session/list`、`session/new`、`session/resume`、`session/prompt`（`text` / `resource_link`）、`session/cancel`、进度推送与双向审批。
  - 服务端可用 provider 范围为已验证的 `CHAT_PROVIDERS`（`codex`、`ply`、`dsh`、`antigravity`）；`claude` 暂不作为被调用端。
  - `loadSession = false`，不宣称 ACP 完整历史重放；正常历史读取仍走原主机连接。
  - `mcpServers` 必须为空，不允许调用方注入外部 MCP server。
- **认证与传输安全**：
  - 网关默认仅监听 loopback（`127.0.0.1` / `::1`）；跨机器经已信任 SSH 端口转发（`BatchMode=yes`、`StrictHostKeyChecking=yes`），provider API 密钥与登录态保留在执行主机。
  - WebSocket upgrade 前校验 Bearer Token；Kite peer 客户端在发送 Bearer Token 前先验证 nonce/HMAC endpoint proof（`x-kite-acp-challenge` / `x-kite-acp-proof`）；拒绝带浏览器 `Origin` 头的请求。
  - 工作目录必须为绝对路径且经 `realpath` 校验落在 `--workspace` 白名单内；Linux 网关必须在用户级 `systemd` 资源层级下运行，以维持 cgroup v2 agent 预算。
- **不重放与有界约束**：
  - 断线、超时或客户端关闭**不得终止远端 owner**，也不得自动重放用户输入、审批或构建；显式取消经 `session/cancel` 转发。
  - 单条消息上限 `2 MiB`、输出排队上限 `4 MiB`、单连接最大并发待处理请求与会话绑定数受硬上限约束，网络层不提供任意 shell RPC。
  - 当前 `kite acp task` 与 `kite acp mcp`（`kite_remote_task`）要求显式指定 `peer`、`cwd`、`provider`（可选 `sessionId`、`expectedRevision`、`allowCommands`），不做自动负载均衡。

---

## 2. 异步会话控制与协作治理（RFC 0033）

### 2.1 控制面与恢复

- 网关在 ACP v1 `initialize` 的 `_meta.kite` 发布 `controlPlaneVersion: 2` 与 `controlMethods`。CLI 和 MCP 提供 `spawn`、`send`、`status`、`read`、`list`、`wait_any`、`cancel`、`release`。Agent 发起的新任务统一使用 v2。同步 `kite acp task` 和 `kite_remote_task` 保留给已有自动化。
- `send` 通过既有 owner 启动一轮，收到 owner 确认后即可返回稳定 `turnId`。可短等至多 30 秒。关闭提交连接或等待超时只结束当前等待。恢复时以 `status` 的活跃或最近轮次和 `read` 返回为准。投递结果不明确时先查事实再决定是否发送。
- `status`、`list` 和 `wait_any` 读取 owner 的活动、交互和终态。单次等待至多 60 秒，支持状态 `revision` 游标。跨 peer 聚合由 CLI/MCP 并发发起，首个就绪结果返回后关闭其他观察连接。MCP 对读取、取消、等待和提交分别限制并发。
- `read` 按轮次取回公开回复与工具摘要，默认预算 64 KiB，响应硬上限 256 KiB。裁剪保留尾部并设置 `truncated`。可从近期持久化投影或有轮次标记的有界原始历史中读取。历史轮次超过窗口时返回不可用。
- `cancel` 按目标会话显式中断当前轮次。空闲可直接返回当前终态。`release` 对活跃任务默认返回保留状态，调用方可显式请求先取消。回收前由既有生命周期适配器验证 owner 身份、空闲状态和持久化，保留目录与原始历史。

### 2.2 目录、工作区和人工介入

- 可选 `delegation` 在目录写入和 CAS 更新间保留，支持按父机器、父会话、标签和状态筛选。单执行主机对每组父机器与父会话最多保留 16 个未释放子会话。旧记录沿用独立会话语义。遇到格式合法但当前适配器不认识的 provider 时，读取隔离该条目，后续目录写回完整保留其原始 JSON。
- `kite acp spawn` 和 `kite_session_spawn` 通过本机目录核实受管会话 ID 或 Codex 原生线程 ID，自动绑定实际父会话。调用参数与托管父会话冲突或托管身份无法核实时，拒绝创建。外部调用方可提供完整父引用；显式创建独立会话时，独立会话选项与父引用互斥。
- 会话和轮次可选模型及思考强度需通过 owner 目录验证。服务档位使用目标 Codex 模型公布的值。附件为执行主机上的本地文件引用，路径和 cwd 均需经过真实路径白名单校验。显式声明写入的会话在同一真实 cwd 竞争写锁时返回冲突，Git 工作区可查询分支与工作树路径。
- `strict` 是默认策略。只有显式白名单中的完整命令及匹配的 cwd 可以获得一次许可。`human_in_the_loop` 要求执行主机上的 Kite Node 可达，原始审批和提问由 owner 保留，复用通知日志和原生交互。`isolated_auto` 校验明确声明的独立 Git linked worktree 及已知活动会话的目录重叠，仅复用目标 provider 已启用并公布的自动档位。
- 远程 `strict` 和 `human_in_the_loop` 轮次显式采用 Antigravity 的常规权限档位，或已公布的 Ply `ask` 档位。兼容的短任务同样采用严格审批。
- Rust Core 将已认证机器身份和会话 ID 用于统一投影 Codex 原生后代及派生会话。离线的派生任务标记为离线。打开目标会话绑定主机与会话后统一发布选中状态。交互本身由用户在目标会话的审批或问答卡上操作。

### 2.3 独立网关的版本

Linux 安装器更新 `kite-node` 后可在安装结果中报告活跃的 `kite-acp.service`。Node 热交接涵盖 Node 的监听器和连接，网关的适配器版本由其独立单元启动时确定。旧网关应在受管任务空闲，且确认单元的停止范围后安排刷新。

验收证据见 [RFC 0033 实施记录](../../rfcs/0033-cross-provider-async-session-control.md#8-实施与验收记录)。
