# kite 工程规范（SPEC）

> 状态：生效 · 更新：2026-11-23（基于 v0.2.4 架构与已有实现整理）
> 适用范围：`packages/` 下的 TypeScript 服务端代码、`crates/` 下的全部 Rust 代码，以及 `android/`、`ios/`、`macos/`、`windows/`、`desktop/` 与 `deploy/` 下的原生与交付代码。

本文是 kite 工程规范的**根目录入口、核心总原则与文档索引**。

- 详细生效规则按主题组织在 [docs/spec/](docs/spec/) 目录下；
- 独立设计提案与历史背景抽离在仓库根目录的 [rfcs/](rfcs/README.md) 目录下；
- 文档措辞中，“**必须**”“**不得**”为硬性要求，“**应当**”为默认做法（例外需在代码或提交说明中写明理由）；
- 规范体系（本文件及 `docs/spec/`）与 `rfcs/` 冲突时，以规范体系为准；已采纳但尚未同步进规范体系的 RFC 视为尚未生效。

---

## 核心架构铁律

以下四条原则贯穿所有端与服务端模块，任何变更不得违背：

1. **Rust 管“做什么”，原生管“怎么呈现”** — 与平台无关的协议、领域状态、持久化和恢复策略只在 Rust 实现一次；原生层专注生命周期、渲染、手势、本地化和安全存储。
2. **状态单向流动** — Rust 持有权威领域状态并推送不可变快照与版本增量；原生层只能通过命令修改状态，不得另存一份平行领域状态。
3. **用户可见文案归原生层** — Rust 只输出结构化状态和稳定错误码，`Display` 仅用于英文诊断日志；面向用户的文案和本地化归各平台原生资源。
4. **媒体与高频输入不走 UniFFI** — 视频帧、桌面音频和实时接管输入走独立媒体通道（Android JNI / Apple & Windows C ABI）。
5. **Kite 不在网络/控制层新增平行模型循环** — 每个 provider 的真实对话循环、凭据、工具执行与持久化上下文始终归目标主机上的唯一 owner 持有；网络断线不得终止远端任务，也不得自动重放用户输入、审批或构建。

---

## 分模块工程规范（`docs/spec/`）

| 规范文档 | 覆盖范围 | 主要依据 RFC |
| --- | --- | --- |
| [系统架构、仓库布局与依赖边界](docs/spec/architecture.md) | 仓库目录规范、Rust crate 分层与依赖规则、Bun 服务端 `packages/*` 单向边界与运行时约定 | [0001](rfcs/0001-architecture.md)、[0024](rfcs/0024-bun-server-monorepo.md) |
| [会话图谱、Provider 与交互](docs/spec/sessions-and-providers.md) | 会话图谱（`ConversationKey` / `Item/Block` / `Thread`）、多主机并发与重连、owner 生命周期与释放、Codex / Claude / DSH / Ply / Antigravity 接入、权限设置、关联任务、异步问题与聊天稳定性 | [0006](rfcs/0006-rich-composer.md)、[0007](rfcs/0007-sessions-and-interactions.md)、[0008](rfcs/0008-native-markdown-and-video-layout.md)、[0009](rfcs/0009-async-questions-in-chat.md)、[0013](rfcs/0013-complete-transcript-and-dark-workspace.md)、[0020](rfcs/0020-session-owner-lifecycle.md)、[0021](rfcs/0021-dsh-provider.md)、[0023](rfcs/0023-harness-capability-parity.md)、[0025](rfcs/0025-ply-acp.md)、[0026](rfcs/0026-session-permissions.md)、[0027](rfcs/0027-conversation-items-and-threads.md)、[0032](rfcs/0032-antigravity-acp.md) |
| [客户端分界、通道与媒体](docs/spec/client-and-media.md) | Rust/原生职责边界、命令/状态/媒体三通道、UniFFI 与 Windows C ABI、桌面串流与混合输入/音频、Android / iOS / macOS / Windows / Linux GTK 平台约束与 M1 兼容基线 | [0002](rfcs/0002-ios.md)、[0003](rfcs/0003-persistent-desktop.md)、[0004](rfcs/0004-chat-desktop-workspace.md)、[0005](rfcs/0005-mobile-operation-and-session-entry.md)、[0010](rfcs/0010-hybrid-input-and-clipboard.md)、[0011](rfcs/0011-compact-desktop-controls.md)、[0015](rfcs/0015-desktop-client-and-host.md)、[0016](rfcs/0016-desktop-audio.md)、[0027-mac](rfcs/0027-macos-client.md)、[0028-keychain](rfcs/0028-macos-keychain-access.md)、[0031](rfcs/0031-windows-client.md) |
| [主机节点、资源预算与运维能力](docs/spec/host-and-operations.md) | Linux/macOS 主机节点、Docker 纯服务端部署、保留连接的 Node 热交接升级、cgroup 共享内存预算、通知与推送、统一 `kite docs` 与 Linux X11 桌面控制 | [0014](rfcs/0014-linux-host-node.md)、[0017](rfcs/0017-agent-notifications.md)、[0018](rfcs/0018-live-node-upgrade.md)、[0019](rfcs/0019-agent-docs-and-linux-desktop.md)、[0022](rfcs/0022-host-resources-and-agent-budget.md)、[0028-docker](rfcs/0028-docker-linux-host.md)、[0029](rfcs/0029-macos-host.md) |
| [网络 ACP 与跨供应商协作](docs/spec/network-acp-and-collaboration.md) | 跨机器统一侧栏、WebSocket `/acp` v1 鉴权与安全边界（已生效）；以及跨供应商长会话异步控制面、血缘归组与人工审批冒泡（RFC 0033 规划中） | [0030](rfcs/0030-network-acp-and-shared-workspaces.md)、[0033](rfcs/0033-cross-provider-async-session-control.md) |
| [规范治理与验收基线](docs/spec/governance.md) | 规范修改流程、RFC 生命周期与编号约定、兼容性原则与分级验收证据要求 | [rfcs/README.md](rfcs/README.md) |

---

## 独立 RFC 规范目录（`rfcs/`）

全部设计提案独立存放在仓库根目录的 [rfcs/](rfcs/README.md)：

- **总索引与流程**：[rfcs/README.md](rfcs/README.md) · [RFC 模板](rfcs/0000-template.md)
- **架构与基线**：[RFC 0001 架构设计](rfcs/0001-architecture.md) · [RFC 0024 Bun 服务端单仓](rfcs/0024-bun-server-monorepo.md) · [RFC 0027 会话图谱](rfcs/0027-conversation-items-and-threads.md)
- **近期关键能力**：[RFC 0030 网络 ACP 与跨机器工作区](rfcs/0030-network-acp-and-shared-workspaces.md) · [RFC 0031 Windows 纯客户端](rfcs/0031-windows-client.md) · [RFC 0032 Antigravity ACP 接入](rfcs/0032-antigravity-acp.md)
- **本次新增设计**：[RFC 0033 跨供应商长会话异步控制面与协作治理](rfcs/0033-cross-provider-async-session-control.md)（草案）

---

## 相关工程与使用文档（`docs/`）

- **文档总目录**：[docs/README.md](docs/README.md)
- **协作开发规范**：
  - [跨供应商长会话协作开发规范](docs/development/cross-provider-collaboration.md)（对应 RFC 0033 的实施边界、代码落点与验收矩阵）
  - [Agent Harness 内部接入规范](docs/development/harness-integration.md)（仅在新增、修改或排查聊天 harness/provider 时按需读取）
  - [Bun 服务端开发说明](docs/development/bun-server.md) · [构建与开发指南](docs/development.md) · [测试规范](docs/testing.md) · [发布与 CI](docs/releases.md)
- **Agent 运行时指南（`kite docs` 源码）**：[docs/agent/index.md](docs/agent/index.md) · [跨机协作指南](docs/agent/collaboration.md)
