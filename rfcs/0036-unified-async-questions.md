# RFC 0036：跨 Provider 统一异步提问表单与多选支持

> 编号：0036 · 状态：已实现 · 创建：2026-11-25
> 相关：[RFC 0009](0009-async-questions-in-chat.md)、[RFC 0023](0023-harness-capability-parity.md)、[RFC 0032](0032-antigravity-acp.md)

## 背景

[RFC 0009](0009-async-questions-in-chat.md) 此前仅识别 Codex 的 `request_user_input_async` 异步提问。Antigravity（`agy acp`）在调用 `ask_question` 时会发送一条 `tool_call`（`title: "Run ask_question"`，`rawInput.questions` 包含 `question`、`options`、`is_multi_select`），随后发送兜底文本并以 `end_turn` 结束本轮，等待下一条 `session/prompt` 回填答案。由于 Kite 服务端控制索引、实时事件、Rust 解析和 FFI 字段均未覆盖 `ask_question` 与 `is_multi_select`，Antigravity 提问在客户端不会弹出原生表单，只能退化为纯文本。

## 目标

- 将 Antigravity 的 `ask_question`（以及任意采用同一结构的受管 harness 工具调用）接入 Kite 现有异步提问模型与原生表单，实现实时弹出、断线重连恢复、侧栏待答状态与通知提醒；
- 扩展统一提问模型与多端客户端表单支持多选（`is_multi_select` / `multi_select`），保持单选、多选与自填一致体验；
- 提交异步提问回答时兼容空闲的受管 `Owner`（`thread/turns/list` 为空与默认沿用当前会话模型/思考/权限设置），并在发送给 `agy acp` 前将回答信封解包为清晰可读的问答文本，同时在 Kite 历史中保留结构化 `Block::QuestionReply`。

**不做的事：**

- 不改变 Codex 现有阻塞/异步提问协议与 TUI 回答信封格式；
- 不改写 provider 原始历史文件。

## 设计

1. **主机服务端（`@kite/providers`）**：
   - `history.ts`：`questionRecords` 索引版本升级为 `5`，同时索引 `request_user_input_async` 与 `ask_question` / `Run ask_question`；
   - `lifecycle.ts`：`notificationFacts` 为 `kite:harness` 会话读取 `history.jsonl` 控制索引，使侧栏待答状态与通知覆盖托管 harness；
   - `owner.ts`：收到 `ask_question` / `Run ask_question` 工具调用时实时推送 `item/completed` 异步提问事件（携带 `providerTool: "ask_question"` 与 `isMultiSelect`）；`thread/turns/list` 在空闲时回报上一终态 turn（冷启动从 `state.json` 恢复），`turn/steer` 与 `turn/start` 在省略 `model` 时回退到当前会话模型、思考强度与权限设置；
   - `native/antigravity.ts`：向 `agy acp` 发送 `session/prompt` 前，将 `<send_user_message_question_reply>` 解包为清晰可读的问答文本，历史记录仍保留原信封供 Kite 渲染为 `QuestionReply` 卡片。
2. **Rust 领域与 FFI（`kite-agent`、`kite-ffi`）**：
   - `interaction.rs`：`Question` 新增 `is_multi_select`（兼容 `isMultiSelect` / `is_multi_select`），校验多选换行拼接答案；
   - `async_input.rs`：支持 `ask_question`（无需等待 `function_call_output` 即进入待答；收到后续用户消息或 `function_call_output` 即解决），支持 `title` / `question` 与 `is_multi_select`；
   - `history.rs`：所有非 Claude 会话统一经 `async_input::history` 解析控制记录；
   - `core_bridge.rs`：`QuestionFieldInfo` 末尾新增带默认值的 `multi_select: bool`。
3. **原生客户端（Android / macOS / Windows）**：
   - `ModelQuestionCard.kt`、`ChatPane.swift`、`MainWindow.cs` 根据 `multi_select` 切换复选框/单选控件，多选结果按换行拼接提交。

## 备选方案

- **仅在客户端正则解析 `agy acp` 的 `- [1] / - [2]` 兜底文本**：脆弱且会丢失 `is_multi_select` 元数据，违背“Rust 解析结构化事实、原生只管呈现”的架构铁律，故否决。

## 对 SPEC 的影响

- 更新 [docs/spec/sessions-and-providers.md](../docs/spec/sessions-and-providers.md) 第 6.2 节，将异步提问与多选表单明确为跨 provider 统一能力（覆盖 Codex 与 Antigravity `ask_question`）。

## 待定问题

- 无。
