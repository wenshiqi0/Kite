# RFC 0032 Antigravity ACP 接入

编号 0032，状态已实现，创建于 2026-11-22。沿用 RFC 0025 的 ACP v1 和 RFC 0020 的托管会话生命周期。

## 主机与客户端

主机通过 `agy acp` 启动 Google Antigravity CLI。执行主机提供 CLI 和登录状态。Kite 使用 `antigravity` 作为 provider ID。新建会话目录要求主机显式启用、ACP v1 握手成功且 `session/list` 可用。这个读取操作验证 CLI 已拾取认证配置。对于 Gemini API key，Google 服务会在模型收到请求时验证凭据。安装和配置见[主机指南](../backend-setup.md#google-antigravity)。

Android、macOS 和 Windows 从主机发布的目录选择 Antigravity，复用同一个 Core 会话和聊天接口。模型及思考强度取自 ACP 的模型清单、模型配置和思考配置。图片输入遵循 ACP 握手能力。模式选项由 `agy` 提供，工具授权使用原请求的允许一次或拒绝一次选项。聊天显示公开文本和工具事件。取消通过 `session/cancel` 向同一会话发送。已有会话也可以通过 Kite 的网络 ACP 连接。

会话权限面板还提供独立的 `tool_permission`：`default` 沿用主机权限设置，`turbo` 对应原生 `agy acp --dangerously-skip-permissions`。Auto 执行模式仍可能请求授权，不能替代 Turbo。这个启动参数由 [Google ACP 文档](https://antigravity.google/docs/ide/extensions/acp#permissions) 和本机 CLI 帮助确认，并非 ACP 公布的模式选项。

选择随下一条消息生效。切换时先等待旧原生进程退出，再以同一原生 sessionId 恢复上下文，保留模型、思考与执行模式；权限选择随恢复记录持久化。排队消息各自保留权限设置，选择 Turbo 不会批准前一轮已挂起的审批，也不改变全局配置或其他会话。已有上下文若不能恢复，停止该轮并保留历史。

## 历史与恢复

Kite 保存主机会话引用、原生 Antigravity 会话 ID 和公开聊天的分页投影。Antigravity 的上下文保留在其自己的存储里。冷启动用 `session/resume` 恢复原有 ID。每次模型轮结束后向 ACP 确认原生会话已可查询。含历史的归档会话只有在确认上下文可恢复后才释放 owner。曾持久化的原生会话若从列表中消失，恢复会报错并保留 Kite 侧历史。

## 验证依据

Google Antigravity CLI 1.5.14 在隔离的本地模型夹具下用于验证握手、登录状态探测、模型配置、会话创建和冷启动后的上下文。主机测试覆盖工具审批、工具结果、取消、投影去重和同一会话恢复。线上 Google 账号与模型服务需要在启用目标主机后验收。 [Google ACP 协议说明](https://antigravity.google/docs/cli/acp).
