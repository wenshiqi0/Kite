# RFC 0032 Antigravity ACP 接入

编号 0032，状态已实现，创建于 2026-11-22。沿用 RFC 0025 的 ACP v1 和 RFC 0020 的托管会话生命周期。

## 主机与客户端

主机通过 `agy acp` 启动 Google Antigravity CLI。执行主机提供 CLI 和登录状态。Kite 使用 `antigravity` 作为 provider ID。新建会话目录要求主机显式启用、ACP v1 握手成功且 `session/list` 可用。这个读取操作验证 CLI 已拾取认证配置。对于 Gemini API key，Google 服务会在模型收到请求时验证凭据。安装和配置见[主机指南](../docs/backend-setup.md#google-antigravity)。

Android、macOS 和 Windows 从主机发布的目录选择 Antigravity，复用同一个 Core 会话和聊天接口。模型及思考强度取自 ACP 的模型清单、模型配置和思考配置（优先保留模型名称里的思考档位标签）。图片输入遵循 ACP 握手能力。模式选项由 `agy` 提供，工具授权使用原请求的允许一次或拒绝一次选项。聊天显示公开文本与工具事件（包含 ACP `diff` 块与任意 JSON 类型的 `rawOutput`）。`agy acp` 推送的 `available_commands_update`（斜杠命令与已加载 Skills）经 `skills/list` 暴露给客户端，发送时把前导 `$skill` 转换为 Antigravity 原生 `/skill` 语法。取消通过 `session/cancel` 向同一会话发送。已有会话也可以通过 Kite 的网络 ACP 连接。

会话权限面板还提供独立的 `tool_permission`：新建会话默认 `turbo`（对应原生 `agy acp --dangerously-skip-permissions`），也可按会话切回 `default` 沿用主机权限设置。Auto 执行模式仍可能请求授权，不能替代 Turbo。这个启动参数由 [Google ACP 文档](https://antigravity.google/docs/ide/extensions/acp#permissions) 和本机 CLI 帮助确认，并非 ACP 公布的模式选项。

选择随下一条消息生效。切换时先等待旧原生进程退出（支持 `session/close` 时先优雅关闭会话），再以同一原生 sessionId 恢复上下文，保留模型、思考与执行模式；权限选择随恢复记录持久化。排队消息各自保留权限设置，选择 Turbo 不会批准前一轮已挂起的审批，也不改变全局配置或其他会话。已有上下文若不能恢复，停止该轮并保留历史。

## 套餐余量监控

Antigravity 复用现有账户额度通道（`account/read` 与 `account/rateLimits/read`，不阻塞聊天轮次）。账号等级取自 ACP 响应 `_meta.antigravity.tier.name`。主机按需调用官方只读命令 `agy -p /usage --output-format json`（不创建对话、不消耗模型轮次），将各模型分组（如 Gemini Models、Claude and GPT models、Gemini 4 Argon）的周期窗口、剩余比例、已用比例与重置时间按原分组顺序投影到统一 `AccountUsage` 模型；使用 Gemini API key 或未登录时显式返回不可用原因，客户端不伪造 0% 或 100%。

## 历史与恢复

Kite 保存主机会话引用、原生 Antigravity 会话 ID 和公开聊天的分页投影。Antigravity 的上下文保留在其自己的存储里。冷启动先经 `session/list` 核实后用 `session/resume` 恢复原有 ID；当 `agy acp` 支持 `session/load` 且历史会话不在 `session/list` 最近窗口内时，改用 `session/load` 校验并恢复原会话（忽略重放通知）。每次模型轮结束后向 ACP 确认原生会话已可查询。含历史的归档会话只有在确认上下文可恢复后才释放 owner。曾持久化的原生会话若既不在列表也不能被 `session/load` 找到，恢复会报错并保留 Kite 侧历史。

## 验证依据

Google Antigravity CLI 1.5.14 在隔离的本地模型夹具下用于验证握手、登录状态探测、模型配置、会话创建和冷启动后的上下文。主机测试覆盖工具审批、工具结果、取消、投影去重和同一会话恢复。线上 Google 账号与模型服务需要在启用目标主机后验收。 [Google ACP 协议说明](https://antigravity.google/docs/cli/acp).
