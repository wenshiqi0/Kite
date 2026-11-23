# 网络 ACP 与跨供应商协作规范

> 所属规范：[SPEC.md](../../SPEC.md) · 状态：第 1 节已生效（v0.2.4）；第 2 节为 RFC 0033 拟增规范（草案，待实现后转正） · 更新：2026-11-23
> 相关提案：[RFC 0030](../../rfcs/0030-network-acp-and-shared-workspaces.md)、[RFC 0033](../../rfcs/0033-cross-provider-async-session-control.md)

## 1. 已生效规则：多机器工作区与网络 ACP 首阶段（RFC 0030）

### 1.1 多机器统一侧栏

- Rust Core 统一发布全部已保存主机的会话目录、能力、连接与选中状态（含离线缓存）。
- 原生侧栏以机器分组展示会话，顶部提供跨机器搜索与新建入口；列表身份由 `(host_id, session_id)` 共同唯一确定。
- 打开会话必须原子绑定目标机器与目标会话，不得先切机器再猜测会话；新建会话必须显式选择目标机器与该机器探测可用的 provider。

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

## 2. 演进规范（草案，对应 RFC 0033，实现前不宣称已交付）

为支撑跨供应商长会话协作与人机共管，[RFC 0033](../../rfcs/0033-cross-provider-async-session-control.md) 定义了下一阶段网络 ACP 异步会话控制面规范；具体代码落点与验收矩阵见 [跨供应商长会话协作开发规范](../development/cross-provider-collaboration.md)。在代码落地并通过验收前，下列能力不得宣称为已实现：

1. **异步会话控制面原语**：在保持“无平行模型循环、断线不重放”前提下，将单次同步阻塞调用扩展为非阻塞 `spawn`、`send`（立即返回 `sessionId` + `turnId`）、只读 `status`、有界 `read`（读取最近一轮或指定轮次的最终回复与工具摘要）、多会话事件聚合 `wait_any`、`cancel` 与基于 `HarnessLifecycle` 的 `release`，并在 `kite acp` CLI 与 `kite acp mcp` 同步暴露。
2. **父子血缘与关联任务收纳**：在主机会话目录与 `_meta.kite` 中持久化可选的父会话血缘（`parent: { machine, sessionId, provider }`、`label`、`role`、`created_by`）；调用方可按父会话检索派生会话，客户端复用“关联任务”平层折叠视图收纳派生会话并支持显式打开/接管。
3. **三档协作权限策略**：显式区分 `strict`（精确白名单，沿用现有默认）、`human_in_the_loop`（审批与提问挂起进入等待态并复用 RFC 0017 通知推送至 Kite 客户端由人类拍板）与 `isolated_auto`（仅在独立隔离工作区/worktree 下允许透传目标 provider 受控自动策略）。
4. **按会话与按轮参数透传**：支持在 `spawn`/`send` 时校验并显式指定 `model`、`effort`、`serviceTier` 与受工作区白名单约束的附件引用。
5. **工作区防踩踏契约**：`status`/`list` 暴露目标工作区当前活跃写入会话与 Git 分支/worktree 信息，写入型并发协作推荐独立 worktree 并在同目录写入冲突时显式拒绝或告警。
