# Agent harness 接入规范

> 范围：内部开发；仅在新增、修改或排查聊天 harness/provider 接入时读取。
> 状态：端到端实施规范；现有实现与待补齐设计分别标明。已实现主机验证的 provider 目录与能力快照；通用 `HarnessAdapter` trait 仍未实现。DSH 与 Ply 的能力对齐见 [RFC 0023](../rfcs/0023-harness-capability-parity.md)，配置按需读取 [DSH](dsh.md) / [Ply](ply.md)。

Kite 已有工程、主机桥接和会话规范，当前分别适配 Codex、Claude、DSH 与 Ply。新增 harness 需要完成相应适配与验收，不能只把名字加入列表。本文不定义新的传输协议；架构与依赖边界以 [SPEC](../../SPEC.md) 为准，引入新外部协议等变更仍按 [RFC 规则](../rfcs/README.md) 处理。

## 1. 先确定接入范围

| 接入目标 | 阅读与改动范围 |
| --- | --- |
| 让某个 harness 调用 Kite 桌面、主机和会话查询工具 | 使用 `kite docs harnesses` 的 CLI/MCP/可选 Skill 合同；无需实现聊天 provider |
| 在 Kite 中发现、启动、查看或恢复该 harness 的会话 | 阅读本文的现有接入点、身份与生命周期约定，再读取受影响模块 |
| 在 Kite 中提供结构化聊天、工具过程和交互 | 在会话接入之上实现消息、运行状态和所声明的交互能力，并分别验收 |

日常使用知识保持在 [Agent 文档](../agent/index.md)。内部适配细节不复制进 `skills/kite/SKILL.md`、常用桌面指南、MCP 初始化说明或工具描述。开发入口只保留链接；具体 provider 的协议、Schema 和故障证据在处理该 provider 时再读取。

### 完整接入的实施顺序

“接入完成”包括服务端可用和客户端可操作。按下列顺序推进，单独增加启动命令或新建会话按钮均不算完成。

| 步骤 | 要交付什么 | 按需深入 |
| --- | --- | --- |
| 1. 定义接入范围 | 稳定 provider ID、实际支持的操作、依赖版本、身份/历史来源、逐平台交付范围 | 本文 §2–3；只查目标 harness 的官方协议或本机 Schema |
| 2. 补齐共享目录与能力状态 | Rust 管理已知 provider、目标主机可用性、当前会话就绪与可用操作；复用现有 `Provider` / `ProviderInfo`，客户端才能按事实呈现 | [客户端合同 §2](harness-client.md#2-先补齐-rust-到客户端的数据合同) |
| 3. 实现主机与领域适配 | 发现、启动/恢复、文本提交、历史/工具事件及所声明的控制能力，接入唯一 owner 和目录存储 | [服务端实施步骤](harness-server.md) |
| 4. 接通 Core 与 FFI | 会话创建/激活、消息同步、能力与阶段快照、操作校验和同版生成的原生绑定 | [服务端步骤 4–5](harness-server.md#4-core-编排与状态发布)、[客户端合同](harness-client.md) |
| 5. 完成客户端展示 | provider 选择、名称/图标、会话与初始化状态、通用聊天、按能力出现的控件、失败/离线/不支持状态 | [客户端页面与行为矩阵](harness-client.md#3-页面与控件展示) |
| 6. 验证并交付 | 服务端夹具、原生展示、真实新会话/首条回复/恢复、已有 provider 回归，以及匹配主机与客户端的发布 | 本文 §5、[客户端验收](harness-client.md#6-客户端验收) |

第 2 步的可用目录与操作元数据已随 DSH 接入实现；后续 provider 复用目录、消息模型和原生组件，新增的是适配器、注册元数据、品牌资源与测试。后续 provider 仍须完成适配与逐平台验收。

## 2. 当前服务端边界与事实来源

调用链是：原生客户端 → Rust Core → SSH → Kite Node → Herdr / 主机适配器 → provider。职责如下：

| 层 | 已有合同与源码 |
| --- | --- |
| 主机入口 | [RFC 0014](../rfcs/0014-linux-host-node.md)：SSH 认证、私有 Unix socket、已知动作；[Node 实现](../../crates/kite-app/src/host_node/mod.rs) 负责配置、参数验证与版本 |
| 桥接传输 | [bridge.rs](../../crates/kite-app/src/host_node/bridge.rs) / [wire.rs](../../crates/kite-app/src/host_node/wire.rs)：保留 stdin/stdout/stderr 和退出码的有界帧，不负责理解 provider 语义 |
| 终端与 agent | [Herdr API 客户端](../../crates/kite-herdr/src/api.rs)：`agent.list`、`agent.start`、`agent.prompt`、`agent.read`、`agent.send_keys` 及 pane/workspace 身份 |
| 主机机制 | [kite_codex.py](../../backend/kite_codex.py)：会话索引原子写入、恢复锁、进程/socket 注册、Codex owner、附件与历史投影；不是通用 provider 插件系统 |
| 会话领域 | [sessions.rs](../../crates/kite-agent/src/sessions.rs)：provider、持久引用、归档、目录修订与合并；[RFC 0007](../rfcs/0007-sessions-and-interactions.md) 规定生命周期 |
| 统一消息与运行状态 | [消息模型和解析](../../crates/kite-agent/src/lib.rs)、[history.rs](../../crates/kite-agent/src/history.rs)、[interaction.rs](../../crates/kite-agent/src/interaction.rs)；[RFC 0013](../rfcs/0013-complete-transcript-and-dark-workspace.md) 规定完整会话和展示边界 |
| 结构化聊天 | [composer.rs](../../crates/kite-agent/src/composer.rs) 与 [Core](../../crates/kite-app/src/core.rs)：当前直接使用 Codex app-server 语义，不能将其方法原样假设为所有 harness 的协议 |

Node API、主机适配器协议、Herdr API、provider 协议是不同的版本边界。以各自实现和对应版本的 Schema 为准，不用某一个版本号替代全部兼容性判断。Herdr 的上游 Schema 定位见 [架构 RFC §4](../rfcs/0001-architecture.md#4-herdr-集成)。

模型、思考强度、输入模态必须从各自后端读取，并经同一 Core/FFI 合同驱动客户端；不能把适配器缺口写成 harness 原生不支持。DSH 和 Ply 的原生协议与环境变量见 [DSH](dsh.md) / [Ply](ply.md)。现有 Codex / Claude 路径如下：

| 能力 | Codex | Claude |
| --- | --- | --- |
| 会话引用、创建与恢复入口 | 已有；复用持久 app-server owner | 已有 provider 分支和 `--resume` 路径 |
| 普通消息提交 | 结构化聊天通道 | Herdr `agent.prompt` |
| 历史与工具消息解析 | Codex 日志投影与解析 | Claude JSONL 解析；当前远端读取为有界尾部，不等同于 Codex 完整历史能力 |
| 模型、思考强度、附件、显式 Skill、额度 | 已有 Codex 专用路径 | 未提供同等的结构化后端路径 |

这张表描述代码路径，不代替真实模型验收。某个名字能被选择、某个进程能启动或夹具能工作，都不能单独证明该 provider 的全部功能可用。现有支持范围同时参照 [测试说明](../testing.md) 和对应发布的验收记录。

## 3. 新增 provider 的行为约定

以下是实现与验收要求，不是已经存在的新 RPC 方法或 Rust trait。适配层将 provider 自己的协议转换为 Kite 的领域模型；原生界面不解析厂商日志、RPC 或终端状态。

### 3.1 身份、发现与能力

- 使用稳定的 provider ID，分别保存 Kite 会话 ID、provider 原始会话引用和当前 pane/terminal 身份。重连或重新启动进程不应改变同一逻辑会话的身份。
- 现有 `AgentSession` 为 `{source, agent, kind, value}`；当前 `kind` 使用 `id` / `path`。新的引用形式必须同时明确定位、校验、恢复与缓存规则，不通过猜测文件名恢复。
- 区分未安装、未连接、初始化中、可用和不支持；只读发现、版本查询与历史读取不得顺带开始模型 turn。
- 在接入说明中逐项声明能力与证据，并按[客户端数据合同](harness-client.md#2-先补齐-rust-到客户端的数据合同)补齐运行时能力状态。当前 `Provider` / `ProviderInfo` 提供本阶段操作元数据；下面清单中尚未接入的能力不能当作已实现 API。

| 能力类别 | 需要说明的行为 |
| --- | --- |
| 会话 | 发现、创建、恢复、归档；能否重启后恢复同一 provider 会话 |
| 消息与历史 | 文本提交、稳定消息 ID、历史完整性、增量/全量读取和实时更新方式 |
| 工具与阶段 | 工具调用/结果关联，开始、结束、失败及等待用户状态的事实来源 |
| 控制 | turn 取消、进程暂停/恢复分别是否支持；两者不能互相冒充 |
| 交互 | 审批、阻塞问题、异步问题；各自的请求身份与响应通道 |
| 可选输入与设置 | 图片/文件/音频、模型、思考强度、Skill、额度；未实现的明确标为不支持或未知 |

新建会话只展示后端验证可用的 provider；缓存历史独立保留。只展示真实支持的操作。终端可用不代表存在结构化审批或问题接口；支持 CLI/MCP 工具也不代表已经接入 Kite 托管聊天。

### 3.2 会话与 owner 生命周期

- 记录、运行进程和连接客户端具有独立生命周期。逻辑归档保留 provider 原始历史与后台任务；关闭面板、断开手机或停止桌面串流不结束 agent。
- 恢复操作使用单会话锁，先核对已有 pane、provider 引用和 owner。已有 owner 暂停、超时或暂时不可达时，不视为不存在并创建第二个 owner。
- 启动请求的响应丢失后，先查询现状；不盲目重放。读取历史可以有界重试，用户消息、回答、审批和控制操作不自动重发。
- 首次会话尚无历史文件是初始化等待状态；临时读取失败不能用空数据覆盖已存在的缓存。就绪判定须来自该 provider 的实际连接能力，不能简单套用 Codex 的条件。

### 3.3 消息、工具与恢复

- provider 原始记录保持权威；适配器负责有界读取/投影，Rust 负责统一消息、合并、完整性和持久缓存。声明历史读取范围；只能读尾部时不宣称完整历史。
- 消息与工具调用使用稳定身份；工具开始、输出、完成和失败按 call ID 关联。实时事件与历史回填合并后不重复显示用户消息、工具或最终回复，正文变化不等于新增消息。
- 显示公开进度、工具过程与最终回复；不根据终端动画或缺失事件虚构回复、成功或隐藏推理内容。
- 声明事件顺序、去重/游标规则，以及重连后的补齐策略；provider 无实时结构化事件时明确采用的替代方式和局限。
- 保持 [RFC 0013](../rfcs/0013-complete-transcript-and-dark-workspace.md) 的 Rust 数据与原生虚拟展示边界；滚动不驱动历史网络请求。

### 3.4 用户交互与运行状态

- 按实际事实映射发送中、思考、执行工具、回复、等待用户、完成/空闲和失败；连接失败、初始化等待与模型失败分别处理，未知状态保持未知。
- 审批与问题分别关联主机、会话及原始 request/call ID；provider 有 turn 身份时一并关联。提交前拒绝过期请求、跨会话响应和已解决请求。
- 普通聊天消息不是审批答复；回答问题不是执行授权。异步问题不自动暂停 agent 或禁用普通聊天，阻塞问题才进入对应等待状态。
- 不支持结构化交互时，明确保留已验证的终端入口；不把任意终端文字猜成可授权的请求，也不把 Chat 自动替换为终端原文。
- 附件、设置与用量必须来自该 provider 的真实能力。不能使用另一 provider 的凭据、默认模型、额度或估算结果伪装支持。

## 4. 现有代码的改动面

| 改动 | 读取与检查 |
| --- | --- |
| 新增会话 provider | `sessions.rs` 的 `observe` / `create`、Python `session_open`、Herdr 对 provider kind 的真实支持、客户端会话选择入口 |
| 共享目录与客户端数据 | Rust provider 注册信息、主机探测、Core `Snapshot`、FFI `CoreSnapshot` / `AgentInfo` / `SessionInfo`；现有 `harness-scan` 发布可用项，客户端使用 `CoreSnapshot.providers` |
| 历史与工具过程 | `history.rs` 的 provider/kind 校验与路径定位、`parse_history`、主机投影、Core 会话同步与缓存；不要只增加一处白名单 |
| 结构化输入与交互 | `composer.rs`、`interaction.rs`、Core 的 provider 分支；按能力接入附件、Skill、额度和通知，不沿用 Codex 的假设 |
| Android 展示与操作 | 新建会话、`providerLabel`、启动就绪、发送锁、模型/附件/Skill/额度、暂停按钮；具体文件和预期见[客户端实施表](harness-client.md#4-android-逐文件实施表) |
| 新主机动作或协议变化 | Node 参数白名单、版本检查、桥接限制与主机安装；按需读取 [RFC 0014](../rfcs/0014-linux-host-node.md) 和 [RFC 0018](../rfcs/0018-live-node-upgrade.md) |

Rust 应用层编排和验证；主机适配器只承接平台机制。继续沿用 SSH 与私有 IPC，不借新增 provider 引入任意 shell RPC、修改全局凭据或接管外部服务生命周期。需要改变这些边界时先走既有 RFC 流程。

实施细节按模块读取：[主机与 Rust](harness-server.md)、[FFI 与原生展示](harness-client.md)。排查启动问题不需要预先加载所有客户端实现；做客户端能力展示也不需要先加载每个 provider 的底层协议。

## 5. 验收与支持声明

按本次声明的接入范围验证：

1. 离线夹具覆盖缺失/不可用能力、引用与路径校验、损坏/部分日志、工具结果关联、重复事件、连接中断和过期交互；不依赖真实账号或外网。
2. 在隔离的自有会话上验证新建 → 首条消息 → 真实回复 → 断开重连 → 同会话继续；核对原始 provider 引用及唯一 owner，不用进程存在代替成功。
3. 对声明支持的工具、审批/问题、取消/暂停、附件和模型设置分别验证；没有做的标为未验证，不用模拟数据充当真实 provider 验收。
4. 复核已有 Codex / Claude 路径，以及历史、设备身份、主机信任与配对的保留。只清理本次自有资源。
5. 更新该 provider 的能力说明、当前实现状态和验收证据。发布与 APK 构建继续遵循 [发布流程](../releases.md)。

完成证据还必须覆盖以下客户端链路，具体测试入口见[客户端验收](harness-client.md#6-客户端验收)：

| 场景 | 客户端可见结果 | 服务端核对 |
| --- | --- | --- |
| 新建目标 harness 会话 | 列出真实 provider，名称/图标正确；初始化期间不能发消息，就绪后可以发送 | 记录中的 provider 与启动/恢复目标一致 |
| 文本、进度与工具 | 首条回复可见；公开进度直接显示，相邻工具折叠；实时事件与历史不产生重复 | 消息/call ID 稳定，工具结果关联完整 |
| 不支持的功能 | 控件按能力隐藏或解释不可用；文本聊天不依赖模型列表、附件或额度接口 | Core 对不支持的操作也明确拒绝 |
| 切换主机/provider/会话 | 草稿、模型、附件、问题和额度不串用；旧会话的晚到事件不覆盖新会话 | 状态发布校验主机、会话和连接代次 |
| 断线、初始化失败、归档与恢复 | 缓存历史可读，状态与重试明确；不因重连自动重发消息 | 恢复同一引用，保留唯一 owner 与原始历史 |
| 未知或当前不可用的 provider | 保留历史和原 ID；不误标 Codex，不启用未经验证的操作 | 未知能力不默认当作支持 |

按修改模块选用 [测试入口](../testing.md)：`cargo test -p kite-herdr -p kite-agent --locked`、`cargo test -p kite-app --features host-node --locked`、`python3 -m unittest discover -s backend -p 'test_*.py'`；客户端展示与真实模型验证另记。只修改本文或索引时，检查链接、源码对应关系和按需加载边界即可，不因此运行模型或打包 APK。
