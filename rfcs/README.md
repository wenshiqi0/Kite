# kite RFC 规范目录

`rfcs/` 独立存放 kite 的全部架构与功能设计提案（RFC）：记录“为什么这样做、权衡过哪些备选方案、边界与验收标准是什么”。

- 仓库根目录 [SPEC.md](../SPEC.md) 是工程规范总入口与目录，分模块生效规则位于 [docs/spec/](../docs/spec/architecture.md)。
- **RFC 讲决策与来龙去脉，SPEC 讲生效结论；两者冲突时以 SPEC 为准。**

---

## 索引

> 历史注记：`0027` 与 `0028` 在早期并行分支中各产生两份历史提案，现保留原文件名避免外部断链；自 `0033` 起严格四位递增、绝不复用。

| 编号 | 标题 | 状态 | 备注 |
| --- | --- | --- | --- |
| [0001](0001-architecture.md) | kite 架构设计 | 已采纳 | 总体分层与三通道基线 |
| [0002](0002-ios.md) | iOS 支持，以及 iPhone 与 iPad 的区分适配 | 已采纳 | 按 size class 适配 |
| [0003](0003-persistent-desktop.md) | 持久 herdr 会话与 Android 远程桌面 | 已实现 | 持久 Core 与 Moonlight 串流 |
| [0004](0004-chat-desktop-workspace.md) | 聊天主界面与浮窗、分屏、全屏、系统画中画 | 已实现 | Android Alpha 2 |
| [0005](0005-mobile-operation-and-session-entry.md) | 手机操作视口、横屏与会话入口 | 已实现 | Android Alpha 5 |
| [0006](0006-rich-composer.md) | 模型、思考强度与附件输入 | 已实现 | Android Alpha 6（含后续速度与额度） |
| [0007](0007-sessions-and-interactions.md) | 会话记录、模型提问与工作过程 | 已实现 | 进程回收由 0020 修订 |
| [0008](0008-native-markdown-and-video-layout.md) | 原生富文本回复与稳定视频布局 | 已实现 | Android Alpha 8 |
| [0009](0009-async-questions-in-chat.md) | 聊天中的异步问题 | 已实现 | Android Alpha 9 |
| [0010](0010-hybrid-input-and-clipboard.md) | 混合鼠标、双向剪贴板与聊天入口 | 已实现 | 混合触控与剪贴板 |
| [0011](0011-compact-desktop-controls.md) | 桌面下方触控板与紧凑鼠标控件 | 已实现 | Alpha 12 |
| [0012](0012-history-pagination.md) | 长会话历史分页与浏览位置保护 | 已被取代 | 已被 [0013](0013-complete-transcript-and-dark-workspace.md) 取代 |
| [0013](0013-complete-transcript-and-dark-workspace.md) | 完整数据、虚拟渲染与深色工作区 | 已实现 | Alpha 13 |
| [0014](0014-linux-host-node.md) | Linux 桌面主机节点、私有桥接与 MCP | 已实现 | 主机 Alpha 17 / 托盘 Alpha 19 |
| [0015](0015-desktop-client-and-host.md) | Linux/macOS 桌面客户端与主机双角色 | 已采纳 | Mac/Win 客户端已实现；Linux 客户端待做 |
| [0016](0016-desktop-audio.md) | Android 远程桌面声音与播放生命周期 | 已实现 | Android Alpha 26 |
| [0017](0017-agent-notifications.md) | agent 通知、系统提醒与会话回访 | 已采纳，实施中 | 后台连接通知已实现；系统推送部署待验收 |
| [0018](0018-live-node-upgrade.md) | 保留连接的 Linux Node 升级 | 已实现 | systemd 热交接 |
| [0019](0019-agent-docs-and-linux-desktop.md) | 统一 Agent 文档、渐进披露与 Linux 桌面操作 | 已实现 | `kite docs` 与 X11 MCP |
| [0020](0020-session-owner-lifecycle.md) | 归档会话资源释放与按需恢复 | 已实现 | 含 0.2.2 共享归档 owner 回收 |
| [0021](0021-dsh-provider.md) | 可用 provider 目录与 DSH 托管会话 | 已实现 | Alpha 45 |
| [0022](0022-host-resources-and-agent-budget.md) | 主机资源概览与共享 Agent 内存预算 | 已实现 | Linux cgroup v2 预算 |
| [0023](0023-harness-capability-parity.md) | Harness 原生能力对齐与统一选择入口 | 已实现 | 统一模型/思考/视觉控件 |
| [0024](0024-bun-server-monorepo.md) | Bun 服务端 packages 与原生 Linux UI 边界 | 已实现 | TypeScript strict 取代 Python 服务端 |
| [0025](0025-ply-acp.md) | Ply ACP 插件接入 | 已实现 | `ply acp` v1 |
| [0026](0026-session-permissions.md) | 会话权限设置与可读审批 | 已实现 | 含 0.2.4 Antigravity Turbo 接入 |
| [0027](0027-conversation-items-and-threads.md) | 会话图谱：Conversation 键、Item 内容块与 Thread 引用 | 已采纳，实施中 | 历史重号（会话领域） |
| [0027](0027-macos-client.md) | macOS 原生远程客户端 | 已实现 | 历史重号（macOS 客户端） |
| [0028](0028-docker-linux-host.md) | Docker Linux 纯服务端部署 | 已实现 | 历史重号（Docker 服务端） |
| [0028](0028-macos-keychain-access.md) | Mac 开发版的持久密钥访问身份 | 已实现 | 历史重号（Mac 钥匙串组件） |
| [0029](0029-macos-host.md) | Mac 客户端内的本机主机角色 | 已采纳，实施中 | launchd 用户节点 |
| [0030](0030-network-acp-and-shared-workspaces.md) | 网络 ACP 与跨机器会话工作区 | 已实现 | WebSocket `/acp` 首阶段 |
| [0031](0031-windows-client.md) | Windows 纯客户端与 Linux 交叉构建 | 已实现 | v0.2.1 正式交付 |
| [0032](0032-antigravity-acp.md) | Antigravity ACP 接入 | 已实现 | v0.2.3 接入 / v0.2.4 Turbo 权限 |
| [0033](0033-cross-provider-async-session-control.md) | 跨供应商长会话异步控制面与协作治理 | 草案 | 配套开发规范见 [cross-provider-collaboration.md](../docs/development/cross-provider-collaboration.md) |
| [0034](0034-resource-aware-session-placement.md) | 会话优先的客户端与资源感知选机 | 已采纳 | 用户可改选执行主机，ACP 派发使用同一评分 |

---

## 什么时候必须先写 RFC

满足以下任一条件，先在 `rfcs/` 写提案，采纳并同步 `SPEC.md` / `docs/spec/` 后再改代码：

- 新增或拆分 Rust crate / Bun package，或改变内部依赖规则。
- 改变 Rust 与原生界面的职责分界，或改变命令/状态/媒体三条通道机制。
- 引入新的外部协议、网络入口、主机侧依赖，或替换核心第三方依赖。
- 影响多个里程碑或跨平台的交互与协作模型设计。

修 bug、在现有规范内补全功能、修正措辞，不需要写 RFC。

## 文件、编号与状态流转

- 文件名：`NNNN-短标题.md`（四位递增编号，从 `0033` 起严格唯一、不复用）。
- 起草方式：复制 [0000-template.md](0000-template.md)，文首注明编号、状态、创建日期和相关 RFC。
- 状态定义与治理细则见 [docs/spec/governance.md](../docs/spec/governance.md)。

| 状态 | 含义 |
| --- | --- |
| 草案 | 起草中，尚未评审定稿 |
| 讨论中 | 等待评审和决定 |
| 已采纳 | 决定按此实施；规范性条目已同步进 `SPEC.md` / `docs/spec/` |
| 已实现 | 已落地代码并通过对应层级验收 |
| 已拒绝 | 决定不实施，保留记录原因 |
| 已被取代 | 被更新 RFC 替代，文首标注替代编号 |
