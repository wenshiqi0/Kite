# 会话权限设置与可读审批

状态：已实现。

## 问题与方案

权限应答只查询 Herdr，遗漏 Kite owner 管理的 Ply/DSH，导致 `agent unavailable`。应答改用统一实时 agent 清单，继续校验主机、会话、原始请求和选项身份，不重发应答。

owner 提供工具名称、完整命令、实际工作目录和结构化参数。Rust 形成公共审批信息；原生显示具体操作、目标和命令，明确允许一次/拒绝。

模型面板展示真实后端支持的会话权限参数。ACP 只投影 `permissions` 类配置；Codex 按本机 schema 和 `configRequirements/read` 约束提供审批策略与文件访问范围。无能力的 provider 不显示选择器。

选择按主机/会话存于现有 chat-settings，随下一条消息传入原生后端。设置不批准当前待确认操作，不修改全局配置、凭据或工具白名单。排队消息保留各自的权限设置，不把新策略混入前一轮。Ply 经 ACP `approval_mode` 持久化 ask/risk/bypass；DSH 只展示实际公布的权限配置。

## 验收与兼容范围

使用本地 SSH/模型/ACP 夹具复现 Herdr 清单缺少托管会话，验证允许、拒绝、过期请求、队列和恢复。验证非法值和未公布值被拒绝，参数到达原生后端，Codex 管理约束在界面中生效。Android 检查卡片、长命令、窄屏和选择提交；发布同版 FFI 与原生绑定。新设置字段缺失时沿用后端原策略；现有消息、审批及恢复身份不变。

依据：本机 codex app-server v2 JSON schema、[官方 app-server 文档](https://learn.chatgpt.com/docs/app-server)、Ply/DSH 当前 ACP 实现。
