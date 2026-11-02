# RFC 0027：会话图谱——Conversation 键、Item 内容块与 Thread 引用

> 编号：0027 · 状态：已采纳，实施中 · 创建：2026-11-02
> 相关：RFC 0007（会话记录）、RFC 0013（完整数据与增量）、RFC 0017（通知）、RFC 0020（owner 生命周期）、RFC 0023（能力对齐）、RFC 0025（Ply ACP）

## 来源与证据

本稿借鉴 Medeo 的 Chat Panel 概念体系，把它映射到 Kite 现有的会话、消息和 provider 抽象上。概念来源：

- `PRD｜Medeo Chat Panel · 以 Session 为核心的改造`（v1.3，html.play.one2x.ai/r/c2IN3bo1_4RU）。**只读到了登录页上的摘要**：Chat Panel 从一条线性聊天改成以 Session 为工作单位、可分叉、可把一段 Session 作为 Chat Thread 放进另一段或另一个 Creation，以及 Chat Item 里能放什么（§4.3 Session · Thread · Item 与两种引用，§3.2 内容块）。全文需要飞书登录，尚未核对。
- 飞书知识库里同一体系的前序文档（已读）：
  - 《Chat and Notification》概念表：Chat Session 是能体与 Room 之间一次有生命周期的连接；Chat Item 是容器，可放入对象；Chat Thread 是多个有关联、可不连续的 Chat Item 的编组，可持久化并作为 Context；Room 是容纳记录的客观容器，有全局唯一 ID；Subscription 是能体与 Room 的持久化个体关系（未读数、已读位置、免打扰、置顶）；Notification 是瞬态事件，应用红点属于 Subscription 而不是通知。
  - 《PRD | Chat Item / Message / 消息》容器内对象类型：Message、Plan、Progress、Task、Chat Item Handler、Tool Call。
  - 《PRD | Chat 产品架构设计》"Room 内部对象结构"：新建的 Session 默认配一个 Room 持久化；Message 支持嵌套并在不同 Session 间同步（外包一层 Message）；一个 Session 可以放到不同 Room（引用转发、拷贝转发）。附图：Chat Item 以 prev/next 链成 Session；从某条 Item 可新建子 Session，形成会话树；虚线表示跨 Session 的 Item 引用；虚框表示 Chat Thread。
  - 《Profile / 配置》Scope 链：User → Organization → Creation → Session → Chat Thread，下层覆盖上层。

Kite 现状的证据见下文各处的 `文件:行号`，均以当前分支为准。

## 背景

Kite 今天的聊天抽象是"一条线性会话 + 一个全局选中"。把上面的概念逐个对照：

| Medeo 概念 | Kite 现状 | 差距 |
|---|---|---|
| Room（持久容器，全局 ID） | `kite_agent::sessions::Record`（[sessions.rs:11-26](../../crates/kite-agent/src/sessions.rs)）只存元数据；消息真相在 provider 历史里 | 身份被 `selected_session` 与 `selected_agent`（terminal_id）两个主键分摊，键有五种写法：`chat_key`（[core.rs:2409](../../crates/kite-app/src/core.rs)）、`source_key`（core.rs:2424）、`history::key`（[history.rs:28](../../crates/kite-agent/src/history.rs)）、`cache_key`、`conversation_context`（[conversation.rs:33](../../crates/kite-app/src/core/conversation.rs)） |
| Session（有生命周期的连接） | 运行态散在 `AppCore`/`Snapshot` 的单例字段上：`history`、`history_window`、`pending_messages`、`composer`、`approval`、`question`、`response_activity`、`agent_pause` | 切换会话要手写同一串清零代码，目前有五份（[core/sessions.rs:451-453、642-644、690-692、742-744、839-841](../../crates/kite-app/src/core/sessions.rs)）；同一时刻只持有一份 transcript |
| Chat Item（容器，放对象） | `Message { id, role, text, intermediate, tool, question_replies }`（[lib.rs:84](../../crates/kite-agent/src/lib.rs)），正文是扁平字符串 | 附件以 `<kite_attachments>` 尾缀拼进 text（[composer.rs:612-618](../../crates/kite-agent/src/composer.rs)），到 FFI 再拆；而发送侧 Codex `turn/start input[]` 和 TS `NativeBlock` 本来就是块 |
| 子 Session / 分叉 | 无。全仓无 fork、parent、origin 字段 | Codex app-server v2 有 `thread/fork`（本机 codex-cli 0.147.0 schema，支持 `lastTurnId` 截到某一轮）；Claude CLI 有 `--fork-session`；ACP（DSH/Ply）没有 fork |
| Chat Thread（可持久化的 Item 编组） | 无 | — |
| 引用转发 / 拷贝转发 | 只有一次性拷贝：`migrationContext` 把旧 harness 历史序列化为纯文本注入新 turn（[harness.ts:313](../../packages/providers/src/harness.ts)、[owner.ts:240](../../packages/providers/src/owner.ts)） | 没有结构、没有来源标记、不可回溯 |
| Subscription（未读、已读位置、置顶、免打扰） | RFC 0017 只有通知事件身份、接收游标和去重；会话列表没有未读与阅读位置 | 红点与通知混在一起 |
| Membership（多个能体同处一个 Room） | 一个会话一个 agent | 本稿不做 |

另一个背景是 `core.rs` 已有 2766 行，`AppCore` 34 个字段、`Snapshot` 33 个字段，会话、对话、审批、问题、composer 全部挂在同一个结构体上且只对应"当前选中"的那一个。多会话功能（Thread、引用、分叉）在这个形态上做，只会再加字段和再加几份清零代码。

## 目标

- 一个会话一个键：所有会话作用域的数据（transcript、缓存、增量上下文、pending、运行态）都从同一个 `ConversationKey` 派生。
- 运行态与持久记录分离：切换会话是替换一个对象，不是清一串字段；Core 可以同时持有多个会话的运行态。
- 消息正文类型化：Item 由块组成，附件、工具调用、问答、嵌入引用都是块，不再用字符串标记承载结构。
- 会话之间有关系：记录来源（从哪条 Item 分叉）、能把一组 Item 保存为 Thread、能把 Thread 以"活引用"或"快照"放进另一个会话。
- 分叉按 provider 真实能力公布，不伪装：Codex 按轮分叉，Claude 整会话分叉，ACP 用快照拷贝。
- 阅读状态（未读、已读位置、置顶、静音）是本机 Subscription，与通知事件分开。

**不做的事：**

- 不移动、不改写 provider 的原始历史文件（RFC 0007/0013 的约束不变）。
- 不做多 agent 同室（Membership）、不做跨主机引用、不引入 Creation 这一层。
- 不改媒体通道，不改 herdr 协议。
- 不为旧 FFI 记录保留兼容入口；FFI 与原生同次生成。

## 设计

### 1. 术语与对象模型

Kite 沿用自己的名字，但一一对应 Medeo 概念：

| Kite 名称 | 对应 | 定义 | 所在 crate |
|---|---|---|---|
| `Conversation` | Room | 一条持久记录：`Record` 原有字段 + `origin: Option<Origin>`。`Record.id` 就是 `ConversationId` | `kite-agent::sessions` |
| `ConversationKey { host, conversation }` | Room 的全局 ID | 所有会话作用域键的唯一来源 | `kite-app::core::key`（新） |
| `LiveSession` | Session | 一个会话的运行态：provider owner/agent 引用、transcript、pending、composer、审批、问题、活动、暂停 | `kite-app::core::live`（新） |
| `Item` | Chat Item | `{ id, role, blocks: Vec<Block>, intermediate, position, origin: Option<ItemOrigin> }`，取代 `Message` | `kite-agent` |
| `Block` | 容器内对象 | `Text`、`Attachment`、`Tool`、`QuestionReply`、`Embed(Reference)`；`Plan/Progress/Task` 留作保留变体，本稿不实现 | `kite-agent` |
| `Thread` | Chat Thread | `{ id, conversation, title, items: Vec<ItemRef>, created_ms }`，有序、可不连续 | `kite-agent::threads`（新） |
| `Reference` | 两种引用 | `Live { conversation, item }` 渲染时解析；`Snapshot { origin, captured_ms, blocks }` 不可变副本 | `kite-agent` |
| `Origin` | 子 Session | `{ conversation, item: Option<ItemId>, turn: Option<String>, mode: Fork \| ContextCopy }` | `kite-agent::sessions` |
| `Subscription` | Subscription | `{ key, read_revision, read_item, pinned, muted }`，本机、按设备 | `kite-store` 文档 |

`Role` 保持 User/Assistant/Tool；工具调用仍按 call_id 合并成一条 Item，`Block::Tool` 承载 `Tool` 的字段。Android 的 Work 分组继续在原生做（RFC 0007），输入从 `MessageInfo` 变成 `ItemInfo`。

### 2. 一个键

`ConversationKey` 是一个小型值类型，提供：`context()`（替代 `conversation_context`）、`cache_document()`（替代 `cache_key`/`history_document`）、`history_key()`（替代 `history::key`，内部仍按 `AgentSession` 推导）。`chat_key`/`source_key` 退化为 `LiveSession` 内部对 agent 的查找，不再对外。`selected_agent` 从 `Snapshot` 移除，改为 `LiveSession.agent`。

规则：任何新函数不得自己拼 `host/…` 字符串。

### 3. LiveSession：把运行态从 AppCore 拆出

```
AppCore
  ├─ hosts / connections / trust / desktop …（不变）
  ├─ book: SessionBook           （持久目录与缓存，不变）
  ├─ live: LiveSessions          （新：BTreeMap<ConversationKey, LiveSession>，有界，默认 4，与 histories 一致）
  └─ selected: Option<ConversationKey>
```

- `LiveSession` 持有今天散落的 `history`、`history_window`、`pending_messages`、`composer`、`approval`、`question`、`response_activity`、`agent_pause`、`turn_revision`，以及它自己的同步任务句柄（RFC 0013 的后台同步按会话隔离、可取消）。
- 切换会话 = `selected` 改指向；被挤出的 `LiveSession` 先落盘再丢弃。`open_session`、`archive_session`、`create_session`、`activate_session`、`sync_sessions` 不再各自清字段。
- `Snapshot` 仍然只发布选中会话的投影，`CoreSnapshot` 的字段先不变（Phase 1 不触碰 FFI），因此 Android 在这一步不改。
- 审批、问题、通知到达时按 `ConversationKey` 路由到对应 `LiveSession`，不再只能命中"当前选中"。非选中会话的审批进入该会话的运行态，并通过 Subscription 变成未读提示，而不是丢弃。

### 4. Item 与 Block

- `kite-agent::parse_history` 直接产出 `Vec<Item>`；id 与 `positions` 排序规则不变（RFC 0013：传输位置不参与 ID）。
- 附件不再拼进文本：解析层把 `<kite_attachments>` 信封解成 `Block::Attachment`，发送层把 `Block` 列表编码到 Codex `turn/start input[]` 和 ACP prompt 的 blocks。provider 历史里 Kite 自有的结构只允许通过一个 `envelope` 模块编解码（现有附件信封保持原样以免迁移数据，新增 `kite_embed` 信封承载快照引用）。已有历史里的旧信封继续能读，这是读取用户数据，不是接口兼容层。
- FFI：`MessageInfo` → `ItemInfo { id, role, blocks: Vec<BlockInfo>, intermediate, origin }`，`BlockInfo` 为 UniFFI enum。`ConversationEditInfo.items` 改为 `Vec<ItemInfo>`。增量协议（`conversation_since`）不变。
- Android：`ChatEntry.Message` 从 blocks 渲染；`ToolGroupCard` 读 `BlockInfo.Tool`；附件卡读 `BlockInfo.Attachment`；新增 `EmbedCard`。

### 5. Origin 与分叉

- `Record.origin` 写入主机目录（TS `SessionRecord` 已允许未知字段原样往返，`packages/protocol` 加显式类型）。
- 能力快照（RFC 0023 的 `Provider`）新增 `fork: ForkSupport { AtTurn, Whole, ContextCopy, None }`，由主机 `harness.ts providers()` 按 provider 公布：
  - Codex：`AtTurn`。主机 `session-open` 新增 `fork` 变体：对 owner 调 `thread/fork { threadId, lastTurnId, cwd }`，用返回的 `thread.id` 建新 `Record`，`origin.mode = Fork`。Item → turn 的映射用已在用的 `thread/turns/list`。
  - Claude：`Whole`。`agent.start --resume <id> --fork-session`，只能从会话末尾分叉。
  - DSH/Ply：`ContextCopy`。新建会话，首个 turn 前置一个 `Block::Embed(Snapshot)`，内容是从源会话截到该 Item 的公开文本；这就是现有 `migrationContext` 的结构化版本，之后 `migrationContext` 合并进同一路径。
- 原生命令：`fork_conversation(host, conversation, at_item) -> ConversationId`。`ForkSupport::None` 时原生不显示入口。分叉出的会话是独立 owner，RFC 0020 的回收规则不变；源会话不受影响。

### 6. Thread 与两种引用

- 存储：主机目录新增 `threads: Vec<ThreadRecord>`，与 `sessions` 同一份文件、同一个修订号和 CAS 写入（[catalog.ts](../../packages/providers/src/catalog.ts)）。目录 1 MB/1024 条上限随之上调或把 threads 拆到 `threads.json`（待定问题 3）。Thread 只引用同一主机上的会话。
- 命令：`create_thread(host, conversation, item_ids, title)`、`rename_thread`、`delete_thread`、`embed_thread(host, target, thread_id, mode)`。
- 两种引用的边界（这是 Kite 与 Medeo 最大的不同，必须写清楚）：
  - **活引用只存在于 Kite 层。** provider 历史是唯一真相且不可改写，所以 `Reference::Live` 只能作为目录里的叠加记录（`ConversationOverlay { conversation, pins: Vec<{ at: Option<ItemId>, reference }> }`），渲染时由 Rust 从源会话的本地缓存或 `history::read` 解析成 blocks。源会话归档或 owner 被回收后仍可读（历史是文件，不依赖 owner）；源会话删除或主机不可达时渲染为"不可用"状态，不报错。
  - **进入 provider 的只能是快照。** 要让 agent 看到另一段会话，就把 `Reference::Snapshot` 作为 `Block::Embed` 放进下一条用户消息，经 `kite_embed` 信封写入 provider 输入；解析历史时还原成 `Block::Embed(Snapshot)`，带 `origin` 以便原生显示"来自 X 的第 N 条"并提供跳转。
- Thread 本身不进入任何 provider；它是 Kite 的编组对象，用途是生成快照、固定活引用、以及后续导出。

### 7. Subscription

- `kite-store` 新增 `subscriptions` 文档：`ConversationKey → { read_revision, read_item, pinned, muted }`，按设备保存，不上主机目录（它是个体关系，不是会话属性）。
- 未读 = 该会话 `conversation_revision` 高于 `read_revision` 且最后一条 Item 不是本机用户发出的。原生打开会话、滚到底部时调用 `mark_read(key, revision, item)`。
- RFC 0017 的通知点击只负责跳转；是否清除红点由 `mark_read` 决定。置顶影响 `views()` 的排序，静音影响通知是否弹出，两者都不改主机目录。
- `SessionInfo` 新增 `unread: bool`、`pinned`、`muted`；FFI 新增 `mark_read`、`set_subscription`。

### 8. 通道与边界

| 通道 | 新增/变化 |
|---|---|
| 命令（原生 → Rust） | `fork_conversation`、`create_thread`、`rename_thread`、`delete_thread`、`embed_thread`、`mark_read`、`set_subscription` |
| 状态（Rust → 原生） | `SessionInfo` 加 `origin`、`unread`、`pinned`、`muted`、`fork_support`；`ItemInfo`/`BlockInfo` 替换 `MessageInfo`；`ThreadInfo` 列表进入 `CoreSnapshot` |
| 主机 CLI/协议 | `catalog-read/-write` 载荷加 `threads`、`overlays`；`session-open` 加 `fork`；`providers` 加 `fork` 能力 |

Rust 仍不产生用户文案；"不可用引用""来自 X 的第 N 条"都是结构化状态，由原生选词。

## 分阶段计划

每个阶段单独可发布、单独验收，顺序不可调换（后一阶段依赖前一阶段的类型）。

| 阶段 | 范围 | 主要改动 | 验收 | 规模 |
|---|---|---|---|---|
| 0 键统一 | `ConversationKey` 落地，删除五种键的拼接；`selected_agent` 改为派生 | `kite-app/core/key.rs`（新）、`core.rs`、`core/sessions.rs`、`core/conversation.rs`、`kite-agent/history.rs` | 现有 Rust 测试全绿；FFI 不变；真机切换会话、重连、归档无回归 | S |
| 1 LiveSession | 运行态从 `AppCore` 拆到 `LiveSession`，有界多会话；审批/问题按键路由 | `core/live.rs`（新）、`core.rs` 瘦身、`core/sessions.rs` 删除五份清零代码、`core/pending.rs` 按会话 | 同上；新增测试：非选中会话收到审批不丢失，切回后可见；`core.rs` 行数下降并记录数字 | M |
| 2 Item/Block | `Message` → `Item` + `Block`；附件走块；FFI `ItemInfo`；Android 按块渲染 | `kite-agent/lib.rs`、`composer.rs`（envelope 模块）、`kite-ffi/core_bridge.rs`、Android `ChatTimeline.kt`、`KiteWorkspace.kt` | RFC 0013 的一万条缓存重开测试通过；附件、生成图片、工具分组、问答回复在真机渲染一致；旧历史里的附件信封仍能读 | M |
| 3 Origin/分叉 | `Record.origin`、`ForkSupport` 能力、Codex `thread/fork`、Claude `--fork-session`、ACP 快照拷贝、Android "从这条开始新会话" | `packages/protocol`、`packages/providers/{harness,codex,owner}.ts`、`kite-agent/providers.rs`、`core/sessions.rs`、Android 会话菜单 | 三类 provider 各一条真实分叉：Codex 截到中间一轮后新会话只含前半段；Claude 整会话分叉；Ply 新会话首轮带结构化快照；能力缺失时入口不出现 | M |
| 4 Thread/引用 | `ThreadRecord`、叠加记录、`Block::Embed`、`kite_embed` 信封、Android 多选→保存 Thread/引用到… | `kite-agent/threads.rs`（新）、`catalog.ts`、`kite-ffi`、Android 抽屉与多选 | 活引用在源会话归档且 owner 回收后仍可渲染；源不可达显示不可用状态；快照进入 provider 后能从历史还原并跳回源 Item；目录 CAS 冲突重试覆盖 threads | L |
| 5 Subscription | 本机阅读状态、未读、置顶、静音，与 RFC 0017 对接 | `kite-store`、`core/sessions.rs views()`、`SessionDrawer.kt` | 通知到达→未读；打开并滚到底→清除；置顶排序持久；静音不弹通知但保留未读 | S |

阶段 0–2 不改变任何用户可见功能，是为 3–5 做的抽象优化；如果只想先收益于"抽象变简单"，可以在阶段 2 之后停下来评估。

## 备选方案

- **活引用也写进 provider 历史。** 需要改写或追加 provider 文件，违反 RFC 0007/0013 的"原始历史不动"；放弃。
- **用目录叠加记录代替 `kite_embed` 信封来标记快照。** 需要在发送后拿到 provider 分配的 Item id 再回写，而本地乐观行与 provider 回显今天靠文本相等对账（`core/pending.rs`），不可靠；信封方案在历史里自描述，放弃叠加方案，只在活引用上用叠加。
- **不拆 LiveSession，直接在 `AppCore` 上加 Thread 和 origin 字段。** 工作量最小，但会把五份清零代码变成六七份，且非选中会话的审批仍会丢；放弃。
- **先做 Thread 再做 Block。** Thread 的快照需要可序列化的块；没有 Block 时只能再造一个字符串格式；放弃。
- **ACP 侧等上游提供 fork。** ACP 当前接口（`session/new|load|prompt|cancel|set_config_option|request_permission`）没有分叉；等待会阻塞阶段 3，先用快照拷贝并把能力标为 `ContextCopy`，不伪装成原生分叉。

## 对 SPEC 的影响

采纳后在 SPEC.md 增加或修改：

- 第 1 节总原则新增术语表：Conversation、LiveSession、Item、Block、Thread、Reference（Live/Snapshot）、Origin、Subscription，并说明与 Medeo 概念的对应。
- 第 3.2 节：`kite-agent` 职责加入"Item/Block 模型、Thread 与引用"；`kite-store` 加入"订阅状态"。
- 第 4.1 节分界表：会话关系（origin、Thread、叠加引用）归 Rust；多选、拖拽、嵌入卡片展示归原生。
- 新增规则：所有会话作用域键必须从 `ConversationKey` 派生；消息正文必须是类型化块，provider 历史中的 Kite 结构只能经 `envelope` 模块编解码；活引用不得进入 provider 历史，进入 provider 的引用必须是带来源的快照；分叉能力按 provider 公布，无能力时不显示入口；阅读状态属于本机 Subscription，不写主机目录。
- 第 4.3 节：`MessageInfo` 由 `ItemInfo`/`BlockInfo` 取代。

## 待定问题

1. **PRD 全文未读。** 本稿的 §3.2 内容块与 §4.3 两种引用的细节来自前序文档与摘要；登录后核对 PRD v1.3 是否对"引用在 Session 间同步"有更强要求（例如双向同步），再决定活引用是否需要变更通知。
2. **Codex `thread/fork` 的分叉粒度是轮（turn），Item 是消息。** 用户选中 assistant 的中间一条时，截到它所在轮的上一轮还是本轮，需要定义并在界面上说明。
3. **目录大小上限。** 现有 1 MB/1024 条（`catalog.ts`）。Thread 条目与叠加记录是否单独文件，取决于真实会话数；先在阶段 4 前用 wen.linux 的目录体积估算。
4. **快照的大小边界。** `kite_embed` 进入 provider 输入会占用模型上下文；需要一个有界截断策略（按字节与条数）并在原生显示"已截断"状态。
5. **有界 LiveSession 的淘汰策略。** 默认 4 个与现有 histories 一致，但带未完成审批的会话不应被淘汰；是否改为"选中 + 有待办的全部保留"。
6. **iOS。** 本稿的原生部分只描述 Android；iOS 在阶段 2 起同步 `ItemInfo` 渲染，不单独排期。
