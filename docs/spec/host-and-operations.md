# 主机节点、资源预算与运维能力规范

> 所属规范：[SPEC.md](../../SPEC.md) · 状态：生效 · 更新：2026-11-23
> 相关提案：[RFC 0014](../../rfcs/0014-linux-host-node.md)、[RFC 0015](../../rfcs/0015-desktop-client-and-host.md)、[RFC 0017](../../rfcs/0017-agent-notifications.md)、[RFC 0018](../../rfcs/0018-live-node-upgrade.md)、[RFC 0019](../../rfcs/0019-agent-docs-and-linux-desktop.md)、[RFC 0022](../../rfcs/0022-host-resources-and-agent-budget.md)、[RFC 0028-docker](../../rfcs/0028-docker-linux-host.md)、[RFC 0029](../../rfcs/0029-macos-host.md)

## 1. 主机节点架构（Linux & macOS）

### 1.1 Linux 主机节点（RFC 0014）

- 使用 `kite-app::host_node` 可选运行时统一承载私有 IPC、服务策略、主机适配器调度与 MCP；`kite-node` CLI 仅处理参数与退出码。
- `kite-desktop`（GTK 面板与常驻托盘）共用同一状态轮询器，仅调用结构化 CLI，不实现服务领域逻辑；面板负责图标、文案、单实例窗口与登录自启动。
- 节点仅复用或启动明确配置的服务，不得停止外部拥有的 provider、Herdr 或 Sunshine；关闭面板或退出托盘不得停止后台节点或运行中的 agent。

### 1.2 macOS 本机主机角色（RFC 0029）

- macOS 节点沿用 SSH、私有 IPC 与既有会话协议，进程机制使用 Darwin API，服务管理走 launchd 用户服务。
- macOS 主机不声明 Linux cgroup 内存硬限额、进程暂停或 X11 桌面 MCP 能力；客户端独立持有机内 Keychain 身份与应用支持目录，不跨设备迁移私钥。

---

## 2. Docker Linux 纯服务端部署（RFC 0028）

- 作为用户级 Linux 安装之外的独立交付路径，Docker 镜像为**纯服务端**：只包含 `kite-node`、Herdr、Sunshine、Bun 以及用于桌面采集的 systemd、Xorg/Xfce、音频、输入法与中文字体；不构建或安装 `kite-desktop` 面板、托盘或客户端。
- 容器外客户端继续使用同一 SSH / Node / 媒体接口及真实 cgroup v2 agent 预算。
- 每台专用 Linux 服务器仅运行一个完整桌面容器实例；镜像不包含任何用户凭据，home 与 SSH 身份独立持久化。
- 容器替换属于维护重启，不得宣称为 Node 的原位无缝热交接。容器脚本位于 `deploy/docker/`，部署命令归 `@kite/linux-host`。

---

## 3. 保留连接的 Linux Node 升级（RFC 0018）

- 采用 `systemd` 持有 listener（`kite-node.socket`）、新代准备就绪后接管新连接、旧代自然排空已有连接的交接方式。
- 主机适配器按源码摘要固定路径释放，确保旧代连接在排空期间始终执行匹配版本的脚本。
- 通知与推送单写者在旧代排空后才启用新代观察任务；交接过程绝不重放用户命令。
- GUI 升级仅提示并等待托盘隐藏空闲或正常重启，不得强制关闭用户正在操作的控制面板。
- 首次从无交接能力的旧版迁移与普通进程重启必须与无缝热交接严格区分，不得混称无缝升级。

---

## 4. 主机资源与共享 Agent 内存预算（RFC 0022）

- Linux 平台适配器（`@kite/platform`）负责采集有界资源事实并落实 cgroup v2 机制；Rust 管理 CPU 差分、内存预警、共享预算与启动预留；客户端主机详情仅负责展示和配置，资源同步独立于聊天通道。
- **预算口径**：
  - 所有受管 provider 及本地工具子进程的实际常驻内存计入共享 agent 预算；
  - 可回收的未映射文件缓存和内核缓存独立展示，**不计入**面板用量、预警、启动准入或调低校验；
  - 控制服务本身不计入 agent 预算；
  - 内核硬上限保持不高于设定预算，随常驻分配自动回收缓存，不因缓存放宽实际内存上限。
- 已有进程迁移保守预留旧内存账目；系统严重压力或未落实限额时拒绝新启动，且不自动重放任务。
- 限额修改原子持久化；调低至当前实际用量以下时拒绝；安全调低前先回收缓存并核验总量。

---

## 5. Agent 通知与移动端回访（RFC 0017）

- Rust 持有通知事件身份、主机持久日志、移动端接收游标、去重逻辑与目标主机/会话验证；原生层负责系统通知渠道、权限与点击导航。
- 点击通知**仅导航打开对应主机与会话**，绝不自动执行审批、跳过问题或发送消息。
- 后台保活连接收到的通知与进程被系统回收后的系统推送（如 FCM）必须分别验收，不能混作同一完成状态。

---

## 6. 统一 Agent 文档与 Linux X11 桌面控制（RFC 0019）

- Agent 能力经统一 `kite docs`、CLI 与标准 MCP（`kite mcp`）暴露，服务任意兼容 harness（Codex、Claude、DSH、Antigravity 等），不绑定特定 harness 私有命名空间或提示词插值。
- 文档正文仅维护一份于 [docs/agent/](../agent/index.md)，薄发现文件位于 [skills/kite/SKILL.md](../../skills/kite/SKILL.md)；按主题及调用方显式传入的 revision 渐进返回，不得维护全局“已读”状态假定。
- Linux X11 桌面动作由应用层校验与选择目标，Bun/TypeScript 平台适配器实现具体观测、输入与 `argv` 级进程启动（禁止经 shell 自动解释命令）。
- 截图保存为私有制品并以标准 PNG 图像块返回，不得用 base64 文本消耗模型上下文；无视觉能力的消费者必须如实声明限制。
