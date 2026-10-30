# RFC 0009：聊天中的异步问题

> 状态：已实现（Android Alpha 9） · 2026-10-30

真实 Codex 0.164.0 会通过 `request_user_input_async` 提问并继续工作。这些问题以 `agentMessage` 的 `delivery: "async"` 与 `questions` 展示，终端将其放在 Queued follow-up inputs；它们不是 `item/tool/requestUserInput` 的阻塞 RPC。Alpha 8 只处理后者，因此聊天仅显示等待提示，不能回答实际排队的问题。

- Rust 将这两类请求归入现有问题模型，但保留各自响应通道。异步问题按原始 call ID 和题目下标关联，选项由真实请求提供，允许自填；不把问题当作执行审批，不预选或自动提交。
- 实时 `agentMessage.questions` 及时生成表单；已有 Codex 历史中的已接受异步请求和用户回答用于重连恢复。用户回答使用安装版 TUI 的 `send_user_message_question_reply` 消息信封，包含准确的 questionItemId、题目和回答，通过同一线程输入。提交被后端接受才移除表单；失败保留草稿，不自动重发未知结果。
- 提问控制记录独立于最近 2 MiB 的聊天显示窗口。主机辅助程序按文件身份和完整行偏移增量索引原始提问调用、其结果及用户回答，不判断是否待答；Rust 决定关联、去重和已答状态。Codex 历史体使用带长度的 v2 分段传输，分别承载控制记录与最近聊天，整体仍有大小上限；主机辅助程序随 APK 同步更新，不保留旧历史体入口。索引为可重建的私有缓存，不修改 provider 历史。运行中的回答使用带实际 turn ID 的 `turn/steer`，轮次已结束时才使用 `turn/start`。
- 已完成 turn 不删除尚未回答的异步问题，显式回答后不因旧快照重新出现。回答在聊天历史中显示为可读文字。原始 provider 历史保持原位。
- 普通的新用户输入视为继续或改变任务，先前问题转入历史；带题目身份的回答只处理指定题目。主机索引保留用户输入边界，普通消息不复制正文和图片，Rust 根据事件顺序过滤已经处理或被替代的旧表单，避免把早已继续执行的旧登录提示再次弹出。
- Android 在聊天中显示选择/自填表单，并在输入区保留可达的“回答问题”入口。异步提问不禁用普通输入，也不暂停 agent；入口可打开同一表单，以便分屏或滚动历史时仍能作答。回答多个问题、失败重试和重连均保留真实请求身份。

继续已确认的升级边界：历史、设备身份、信任、配对保留，Rust/FFI/APK 同步生成，不维护旧 FFI。无需改变主机服务、模型配置或全局 Codex 进程。

依据：[官方 App Server 文档](https://learn.chatgpt.com/docs/app-server) 的原生问题 RPC；异步变体以远端安装版 `codex app-server generate-json-schema --experimental` 的 `AgentMessageThreadItem`/`AsyncUserInputQuestion`、真实历史记录及安装版 TUI 的回答信封为准，使用独立测试会话验证，不替用户回答正在处理的真实问题。
