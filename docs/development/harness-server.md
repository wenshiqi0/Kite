# Harness 接入：主机与 Rust 实施步骤

> 仅在实现或排查聊天 provider 的主机/领域接入时读取。入口、通用约定和完成标准见[接入规范](harness-integration.md)。

下面按当前源码列出新增 provider `acme` 的完整改动路径。`acme` 是占位 ID，不代表已经支持的产品；可用目录与能力状态按[客户端合同](harness-client.md#2-先补齐-rust-到客户端的数据合同)实施，不存在可直接调用的 `provider.register` 命令。

## 1. 固定目标协议与运行方式

先在该 provider 的接入说明中记录：CLI/SDK 版本、发现与认证状态如何只读查询、创建/恢复参数、会话引用、原始历史位置或读取接口、消息与工具事件、可用交互和退出行为。只读取目标 provider 的协议资料，不批量加载其他 harness 文档。

选择实际成立的一条运行路径：

| 路径 | 实施内容 | 不足时的处理 |
| --- | --- | --- |
| Herdr 管理的 TUI | 验证所用 Herdr 版本真实接受 `agent.start.kind=acme`，能发现稳定 `agent_session`、接收 `agent.prompt` 并读取状态；补 Kite 的历史/能力适配 | Herdr 不支持该 kind 时，先实现上游集成或另立主机 owner 路径；只修改 Kite 白名单不能使它可用 |
| provider 自有结构化服务 | 在主机管理明确归属的进程/socket，将其协议适配到 Kite 消息、运行状态和操作；TUI 与 Kite 若共享一个会话，应连接同一 owner | 不要求 provider 实现 Codex 的 `thread/*`、`turn/*`；无等价能力时明确声明限制 |

只有进程和终端输入、没有可关联的会话与历史时，最多声明终端接入；不能把它列为结构化聊天已完成。新增外部协议或核心依赖按既有 RFC 规则评估。

## 2. 接通创建、发现与恢复

按以下顺序修改并验证：

1. 在 Rust 的共享 provider 定义和主机探测中注册 `acme`、它的静态能力和原生品牌资源标识。主机探测只返回安装/连接/版本等事实，Core 得到当前主机的目录与可用操作；不靠 Kotlin 名单决定服务端是否接受该 provider。
2. 更新 [Repository::create / observe](../../crates/kite-agent/src/sessions.rs)：现有过滤通过 `providers::registered` 接受 Codex、Claude 和 Kite 管理的 DSH；新增 provider 时同步注册和来源校验；保留 `Record.provider`、`reference` 和已有目录 CAS 行为。`Record::matches` 继续按原始引用或明确的待启动 terminal 身份关联，不能只比 cwd 或标题。
3. 为 [session_open](../../backend/kite_codex.py) 实现 `acme` 的安装检查、创建/恢复参数与 owner 机制。该函数先分派 DSH 的 SDK owner，legacy 白名单只接受 Codex/Claude，`else` 是 Claude 的恢复逻辑；新增值必须显式分派，不能直接落入 Claude 分支。使用原有单会话锁、已有 owner 检查和响应丢失后的状态查询。
4. 返回真实 `Agent` / `AgentSession`。确认 `source`、`agent`、`kind`、`value` 的来源与有效性；原始 provider ID 不得被替换为 Codex，也不能把 pane ID 当作 provider 会话 ID。
5. 在 [Core::create_session](../../crates/kite-app/src/core/sessions.rs) 更新 provider 校验及激活策略。当前 Codex 与 DSH 在创建后自动 `activate_session`，Claude 是待连接记录；新 provider 必须明确“创建即连接”还是“仅存草稿”。完整可用的新会话默认应完成实际激活，若仅支持草稿则界面明确提供连接入口。
6. 分别验证 `open_session` 的历史查看与 `resume_session` 的实际恢复。查看记录不自动发送 prompt；归档不删除原始历史、不结束 agent。

关闭、退出和重连还需检查 [process.rs](../../crates/kite-agent/src/process.rs)：当前进程定位只识别 Codex/Claude。只有验证过 provider 的真实进程身份与暂停机制，才向 Core 暴露暂停能力；不把终止、取消 turn、暂停进程当作同一操作。

## 3. 实现历史、事件与输入映射

| 项目 | 当前入口 | 对 `acme` 要完成的改动 |
| --- | --- | --- |
| 历史定位与读取 | [history.rs](../../crates/kite-agent/src/history.rs) 的 `request` / `page` | 替换双 provider/kind 过滤与 Codex/Claude 路径假设；明确来源、大小边界、完整性与游标。新增路径不能落入 Codex 的默认 `else` |
| 统一消息 | [parse_history](../../crates/kite-agent/src/lib.rs) | 将公开文本映射为稳定 `Message.id` / `Role`，工具调用映射为 `Tool` 的参数、输出、完成、失败与 computer 标志；call ID 关联结果 |
| 完整同步 | [core/conversation.rs](../../crates/kite-app/src/core/conversation.rs) 的 `ensure_history_sync` | 当前为 Codex 与 DSH 补齐历史；支持完整读取的新 provider 必须接入对应策略。只有尾部读取时发布真实限制，不把 `complete` 置为真 |
| 实时输入与事件 | [composer.rs](../../crates/kite-agent/src/composer.rs)、[interaction.rs](../../crates/kite-agent/src/interaction.rs) | provider 原始协议在领域适配边界转换；不将 Codex RPC 发给 `acme`。事件与历史补齐按稳定 ID 合并 |
| 附件、模型、Skill、额度 | [core/skills.rs](../../crates/kite-app/src/core/skills.rs)、[usage.rs](../../crates/kite-agent/src/usage.rs) 及 composer/Core 对应命令 | 仅实现声明支持的能力；文件和工件引用限定目标主机/会话，设置及用量也按该范围隔离 |
| 通知 | [core/notifications.rs](../../crates/kite-app/src/core/notifications.rs)、[Node notifications](../../crates/kite-app/src/host_node/notifications.rs) | 将该 provider 的真实完成/待处理事实接入已有日志与去重，不能沿用仅对 Codex 成立的控制记录解析 |

纯文本路径可使用经过验证的 Herdr `agent.prompt`。结构化路径需为 `acme` 实现自己的连接、就绪、提交和响应映射，再供 Core 调用；不能因为它不是 Codex 就默认它能安全走终端输入。

审批、阻塞问题、异步问题都保留原始请求身份。没有相应结构化协议时，明确不支持或提供已经验收的终端操作入口，不能从普通回复文字构造授权卡。发送失败保留草稿；结果未知时先恢复事实，不能自动重发。

## 4. Core 编排与状态发布

检查 [core.rs](../../crates/kite-app/src/core.rs) 的 `ensure_agent`、`refresh_agent`、`refresh_composer` 和 `prompt_agent`，以及 [core/sessions.rs](../../crates/kite-app/src/core/sessions.rs) 的激活与切换：

- `ensure_agent` 的当前缺省是 Codex；新增一个 provider 不应悄悄改变已有用户的缺省。自动准备与用户显式选择必须使用相同的 provider 校验和生命周期逻辑。
- `refresh_composer` 当前连接 `providers::managed` 的 Codex/DSH `id` 引用；新增 provider 的文本就绪、模型目录和附件能力不能依赖这段 Codex 握手全部成功。
- `prompt_agent` 当前分为 managed provider 的规范化提交与 legacy agent 的 Herdr 文本提交。将注册 provider 路由到经验证的实现，按实际能力检查文本、附件和 Skill，再提交到当前会话。
- 创建/切换会话先重置上条会话的 composer、问题、审批和阶段；异步发布核对 host、session/reference 与连接代次。草稿与设置按会话保存，不能用上一个 provider 的 ready 状态解锁本次发送。
- 发布[客户端数据合同](harness-client.md#2-先补齐-rust-到客户端的数据合同)所需的目录、可用性、就绪、可用操作、历史与运行状态；同一份事实用于命令校验。客户端禁用按钮不是服务端校验的替代品。

## 5. FFI、主机包与客户端接线

[core_bridge.rs](../../crates/kite-ffi/src/core_bridge.rs) 目前通过 `create_session(provider, cwd, title)` 传 provider 字符串，通过 `SessionInfo` 返回保存的 provider；现已增加 `providers` 目录，`AgentInfo.can_pause` 映射领域操作元数据。

补齐 Core 的领域数据后，将新增数据一对一映射到 FFI，暂停等能力判断移回领域/应用层。调用端继续通过 Core 命令和 observer 获取状态；不让 Kotlin/Swift 读主机文件、猜版本或直接调用 provider SDK。Rust 库与原生绑定在同次构建生成，生成物不手改，参见[开发指南](../development.md)。

如果增加 Node 主机动作，在 [host_node/mod.rs](../../crates/kite-app/src/host_node/mod.rs) 的参数白名单、版本检查和安装内容中同步处理，并保留有界桥接。只改 Rust 本地快照不等于一定要升级 Node 协议；按实际兼容面判定，不添加未经要求的旧版回退。新主机文件也要进入 [package.py](../../desktop/linux/package.py) 的打包输入和对应安装逻辑。

完成这里后继续做[客户端展示](harness-client.md)，不能只用 CLI 测通就宣称接入完成。

## 6. 服务端完成核对

- 自有 `acme` 会话通过创建、首条真实回复、工具结果、断线后同引用恢复；相同记录多次打开保持唯一 owner。
- 缺失 CLI、版本不支持、尚未初始化、历史尚未生成、进程退出分别给出准确状态，不自动重放结果未知的提交。
- 文本、工具与历史中的稳定 ID 经过实时/历史合并验证，已有 Codex/Claude 保持原行为。
- FFI 返回客户端所需的实际元数据和能力，UI 能按下一页的矩阵操作；测试证据区分夹具、真实模型和原生展示。
