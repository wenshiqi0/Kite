# kite RFC

RFC 记录 kite 的设计决定：为什么这么做、考虑过哪些别的方案、对现有规则有什么影响。

- 当前生效的硬性规则汇总在仓库根目录的 [SPEC.md](../../SPEC.md)。
- RFC 讲来龙去脉，SPEC 讲结论。两者冲突时以 SPEC 为准。

## 索引

| 编号 | 标题 | 状态 |
|---|---|---|
| [0001](0001-architecture.md) | kite 架构设计 | 已采纳 |
| [0002](0002-ios.md) | iOS 支持，以及 iPhone 与 iPad 的区分适配 | 已采纳 |
| [0003](0003-persistent-desktop.md) | 持久 herdr 会话与 Android 远程桌面 | 已实现 |
| [0004](0004-chat-desktop-workspace.md) | 聊天主界面与浮窗、分屏、全屏、系统画中画 | 已实现（Android Alpha 2） |

| [0005](0005-mobile-operation-and-session-entry.md) | 手机操作视口、横屏与会话入口 | 已实现（Android Alpha 5） |
| [0006](0006-rich-composer.md) | 模型、思考强度与附件输入 | 已实现（Android Alpha 6） |
| [0007](0007-sessions-and-interactions.md) | 会话记录、模型提问与工作过程 | 已实现（Android Alpha 7） |
| [0008](0008-native-markdown-and-video-layout.md) | 原生富文本回复与稳定视频布局 | 已实现（Android Alpha 8） |
| [0009](0009-async-questions-in-chat.md) | 聊天中的异步问题 | 已实现（Android Alpha 9） |
| [0010](0010-hybrid-input-and-clipboard.md) | 混合鼠标、双向剪贴板与聊天入口 | 已采纳 |
| [0011](0011-compact-desktop-controls.md) | 桌面下方触控板与紧凑鼠标控件 | 已实现 |
| [0012](0012-history-pagination.md) | 长会话历史分页与浏览位置保护 | 已被 0013 取代 |
| [0013](0013-complete-transcript-and-dark-workspace.md) | 完整数据、虚拟渲染与深色工作区 | 已实现（Alpha 13） |
| [0014](0014-linux-host-node.md) | Linux 桌面主机节点、私有桥接与 MCP | 已实现（Alpha 17） |
| [0015](0015-desktop-client-and-host.md) | Linux/macOS 桌面客户端与主机双角色 | 已采纳；Mac 客户端技术验收，其他阶段待实现 |
| [0016](0016-desktop-audio.md) | Android 远程桌面声音与播放生命周期 | 已实现（Android Alpha 26） |
| [0017](0017-agent-notifications.md) | agent 通知、系统提醒与会话回访 | 已采纳，实施中 |
| [0018](0018-live-node-upgrade.md) | 保留连接的 Linux Node 升级 | 已实现，尚未发布 |
| [0019](0019-agent-docs-and-linux-desktop.md) | 统一 Agent 文档、渐进披露与 Linux 桌面操作 | 已实现，待发布 |
| [0020](0020-session-owner-lifecycle.md) | 闲置会话进程回收与按需恢复 | 已实现 |
| [0021](0021-dsh-provider.md) | 可用 provider 目录与 DSH 托管会话 | 已实现（Alpha 45） |
| [0022](0022-host-resources-and-agent-budget.md) | 主机资源概览与共享 Agent 内存预算 | 已实现 |
| [0023](0023-harness-capability-parity.md) | Harness 原生能力与统一客户端展示 | 已实现 |

| [0025](0025-ply-acp.md) | Ply ACP 插件接入 | 已实现，待发布 |
| [0027](0027-conversation-items-and-threads.md) | 会话图谱：Conversation 键、Item 内容块与 Thread 引用 | 已采纳，实施中 |

## 什么时候写 RFC

- [0030 — 网络 ACP 与跨机器会话工作区](0030-network-acp-and-shared-workspaces.md)（已采纳，实施中）

下面这些改动，先写 RFC：

- 新增或拆分 crate，改变依赖规则。
- 改变 Rust 与原生界面的分界，或者改变三条通道（命令、状态、媒体）的机制。
- 引入新的外部协议、主机侧依赖，或者新的第三方核心依赖（例如换 SSH 库、换串流实现）。
- 影响多个里程碑的交互设计。

修 bug、在现有规则内实现功能、改措辞，不需要 RFC。

## 文件和编号

- 文件名：`NNNN-短标题.md`，例如 `0002-chat-panel.md`。编号四位，从 0001 起依次递增，不复用。
- 新 RFC 复制 [0000-template.md](0000-template.md) 开始写。
- 每份 RFC 开头写明编号、状态和创建日期。

## 状态

| 状态 | 含义 |
|---|---|
| 草案 | 正在写，还没开始讨论 |
| 讨论中 | 等待评审和决定 |
| 已采纳 | 决定按它做；涉及规则的部分已同步进 SPEC.md |
| 已实现 | 已经落地到代码 |
| 已拒绝 | 决定不做，保留文档记录原因 |
| 已被取代 | 被更新的 RFC 替代，开头注明被哪一份取代 |

## 流程

1. 复制模板，取下一个编号，状态写"草案"。
2. 写完后改成"讨论中"，在索引里登记。
3. 决定采纳时：状态改成"已采纳"；如果改变了规则，在同一个改动里更新 SPEC.md。
4. 代码落地后改成"已实现"。

已采纳的 RFC 不再大改。要推翻或大幅修改，写一份新的 RFC，把旧的标成"已被取代"。

- [0024 — Bun server packages and native Linux UI](0024-bun-server-monorepo.md)
- [0026 — 会话权限设置与可读审批](0026-session-permissions.md)
- [0027 — macOS 原生远程客户端](0027-macos-client.md)
- [0028 — Mac 开发版的持久密钥访问身份](0028-macos-keychain-access.md)
- [0029 — Mac 客户端内的本机主机角色](0029-macos-host.md)
