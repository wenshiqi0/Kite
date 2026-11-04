# kite 工程规范（SPEC）

> 状态：生效 · 2026-10-28
> 适用范围：`packages/` 下的 TypeScript 服务端代码、`crates/` 下的全部 Rust 代码，以及 `android/`、`ios/` 与 `desktop/` 下的原生代码。
> 本文汇总当前生效的硬性规则。每条规则的来龙去脉记录在 [docs/rfcs/](docs/rfcs/README.md) 里，整体设计见 [RFC 0001](docs/rfcs/0001-architecture.md)。要调整 crate 划分，或者调整 Rust 与原生界面的分界，按第 7 节的流程先写 RFC、改这份规范，再改代码。

文中"必须""不得"是硬性要求，"应当"是默认做法，有充分理由时可以例外，但要在代码或提交说明里写明理由。

## 1. 总原则

Codex 子 agent 作为父会话下的树展示，保留 provider 线程 ID 与直接父线程 ID，支持多级展开与折叠。主机通过现有 owner 的只读 `thread/list` 获取后代元数据与原生运行事实；Rust 统一决定运行、等待、完成、失败、中断与未知状态，原生层负责树的渲染。已记录的子树随父会话保存；缺少实时事实时不得沿用“运行中”。子 agent 的发现不创建独立终端或 owner，不改变父会话的聊天引用及生命周期。

1. **Rust 管"做什么"，原生管"怎么呈现"。** 与平台无关、各平台都需要的逻辑，在 Rust 里只实现一次。
2. **状态单向流动。** Rust 持有领域状态，向原生推送不可变的快照；原生只能通过调用命令改变状态，不得自己另存一份领域状态。
3. **用户看到的文字由原生层负责。** Rust 只交出结构化数据，文案和本地化在原生层。
4. **媒体数据不走 UniFFI。** 视频帧和高频输入走专门的媒体通道（见 4.4）。

聊天工作区以 [2026-10-29 产品稿](docs/product/2026-10-29/README.md) 和 [RFC 0004](docs/rfcs/0004-chat-desktop-workspace.md) 为交互依据：接管与 agent 暂停独立；观看缩放不发送输入；布局和 PiP 由原生层处理，真实会话、审批与暂停由 Rust 处理。

手机操作按 [RFC 0005](docs/rfcs/0005-mobile-operation-and-session-entry.md) 使用统一源桌面坐标系和显式只看/操作模式；连接阶段与会话启动仍由 Rust Core 管理。

聊天模型、思考强度和附件按 [RFC 0006](docs/rfcs/0006-rich-composer.md) 使用 Codex app-server v2：同一 pane 的 TUI 和手机连接同一持久服务器，保留 thread ID。`kite-agent` 持有结构化聊天协议和有界附件传输，原生仅提供私有缓存文件路径和展示。模型目录、设置、上传状态归 Rust；普通文件及不支持原生音频时通过主机路径提供给 agent。Linux 辅助程序位于 `packages/`，不得修改全局 Codex 服务或凭据。

Android 输入区不提供语音识别入口。空草稿时，正在工作的 agent 显示中断按钮；初始化和空闲时显示正常发送按钮，未就绪或没有内容时禁用。草稿包含文字、附件或 Skill 时仍可发送追加指令。中断通过 Rust 校验当前会话并取消实际活动轮次，终端型 agent 使用已验证的 Escape 输入；保留会话和草稿，与进程暂停分开。

显式 Skill 选择沿用同一协议与状态边界：Rust 按主机和会话持有目录及本条消息的选择，通过 `skills/list` 按 cwd 查询，发送原生 skill 输入；Android 在加号菜单提供名称/简介列表和可移除标签。发送成功清除选择，失败保留；不修改全局 skill 配置。

套餐额度同样按 RFC 0006 从当前会话 owner 只读查询；Rust 负责窗口、剩余百分比、更新代次与过期状态，原生在输入框左侧模型选择面板展示。查询任务独立于聊天且随客户端销毁取消，不保存账户标识或凭据，不用缺失字段猜测余额，不通过重置时间推断可发送权限。

会话、归档、模型提问和响应阶段按 [RFC 0007](docs/rfcs/0007-sessions-and-interactions.md) 管理：Rust 持有持久记录、提供方、选中会话、回答关联和运行阶段；主机会话索引使用原子修订号写入，辅助程序承担单会话恢复租约及 Linux 终端启动。原始 provider 历史不移动。进程生命周期按 [RFC 0020](docs/rfcs/0020-session-owner-lifecycle.md)：归档保留执行中的任务，任务结束落盘后回收 TUI、app-server 及其子进程；未归档会话不因闲置超时回收。资源释放是托管 harness 的统一协议能力；客户端与自动归档使用同一个结构化命令，由适配器验证持久化上下文并释放自身 owner 及本地子进程。继续会话按原 provider thread 恢复。Android 负责相邻工具调用折叠、公开文本直显、题目输入及阶段动画，Claude 未连接时明确显示待连接状态。

新增聊天 provider 按 [RFC 0021](docs/rfcs/0021-dsh-provider.md) 使用主机验证过的可用目录和 Rust 能力快照驱动客户端选项。Ply 按 [RFC 0025](docs/rfcs/0025-ply-acp.md) 使用独立 ACP 插件的会话、配置、工具事件、权限和取消接口；恢复只保存原生 sessionId，私有快照留在 Ply。DSH/Ply 需主机环境变量显式启用并通过鉴权模型服务与原生协议探测；[RFC 0023](docs/rfcs/0023-harness-capability-parity.md) 规定原生模型、思考强度和视觉能力贯通同一客户端控件，harness 使用下拉框选择；细节仅在接入或排查时读取[内部实施规范](docs/development/harness-integration.md)。

会话权限设置按 [RFC 0026](docs/rfcs/0026-session-permissions.md) 放在模型浮层：只展示原生后端公布或本机协议及管理约束允许的参数，按主机和会话保存，随下一条消息生效。权限应答使用统一实时 agent 清单，涵盖独立 owner 托管会话；审批卡展示具体操作、完整命令、工作目录和结构化参数，允许一次不改变会话策略。

显式删除会话按 RFC 0020：活动和归档列表都可直接删除，命令绑定目标主机及会话。删除授权立即终止该会话的任务；主机在会话、owner 和目录租约内验证身份、排除共享 owner，释放受管进程及其本地工具子进程，确认退出后才移除 Kite 记录。本机清除聊天缓存和选中状态，选择下一条活动会话或空列表；释放失败保留记录以供重试。私有删除标记阻止过期目录写入或发现结果复活记录，不保存对话内容；provider 原始历史文件保持原位。

会话图谱按 [RFC 0027](docs/rfcs/0027-conversation-items-and-threads.md) 组织：Conversation 是持久记录，LiveSession 是一个会话的运行态，Item 由类型化 Block 组成，Thread 是有序、可不连续的 Item 编组，Reference 分活引用与快照，Origin 记录分叉来源，Subscription 是本机阅读状态。硬性规则：所有会话作用域的键必须从 `ConversationKey` 派生，不得另拼字符串；切换会话是替换运行态对象，不得逐字段清零；消息正文必须是类型化块，provider 历史中的 Kite 自有结构只能经统一 envelope 模块编解码；活引用只存在于 Kite 目录叠加层，进入 provider 的引用必须是带来源的快照；分叉能力按 provider 真实公布（Codex 按轮、Claude 整会话、ACP 快照拷贝），无能力时不显示入口；阅读状态、置顶与静音属于本机 Subscription，不写主机目录；原始 provider 历史仍不移动。分阶段实施顺序见该 RFC。

模型 Markdown 的原生富文本展示及视频测量按 [RFC 0008](docs/rfcs/0008-native-markdown-and-video-layout.md) 实现：Android 使用 Markwon 绘制 assistant 回复与公开进度，保存的原文和后端协议不变；视频子平面在 measure/layout 中同步定尺寸，显示与输入继续共用源桌面坐标系。Codex 全局 `AGENTS.md` 注入消息在 Android 仅显示一行“已加载全局指令”；首次加载及重新加载使用相同展示。识别仅限完整的全局指令信封及其可选环境上下文，普通消息、仓库级指令和信封后的用户请求保持正文展示；原始对话数据不变。

聊天中的异步问题按 [RFC 0009](docs/rfcs/0009-async-questions-in-chat.md) 处理：Rust 关联真实 call ID、恢复待答表单，并通过同一线程的 TUI 回答信封提交；阻塞问题继续使用其 RPC 响应。提问控制记录通过同步升级的 Codex 历史体 v2 独立传输，主机只索引原始事实，Rust 决定待答状态，避免最近聊天窗口截断旧问题。Android 在聊天和输入区提供回答入口，异步问题不暂停 agent 或禁用普通消息。

Android 混合输入和双向文本剪贴板按 [RFC 0010](docs/rfcs/0010-hybrid-input-and-clipboard.md) 实现：触屏绝对定位与仿真鼠标相对移动并存；鼠标按键、滚轮和本地视口手势分开处理。远端系统剪贴板由 Rust 经 SSH 操作，本机剪贴板归原生；限主机身份和当前操作会话、大小与超时，不记录内容。代码和审批命令保留原文，选项展示样式不改变提交值。

紧凑桌面控件按 [RFC 0011](docs/rfcs/0011-compact-desktop-controls.md) 排列为桌面、可收起触控板、Chat与底部输入框；横屏鼠标附着于整个工作区。面板和键盘由原生互斥管理，保持源坐标映射与单解码器。

长会话按 [RFC 0013](docs/rfcs/0013-complete-transcript-and-dark-workspace.md) 独立同步并保留完整数据。Rust 管理完整消息、持久缓存与同步，状态快照不携带正文，原生通过版本增量维护展示镜像。原生只管理虚拟渲染窗口、滚动和跟随最新，滚动不得触发历史网络请求或淘汰逻辑消息。全屏保留鼠标形状控件与右侧操作栏，触控板只用于上下分屏；界面使用统一深色配色。

全屏鼠标展开后，箭头与左右键控件保持固定的屏幕相对位置，缩放和平移不改变这个偏移；每次操作使用 DesktopViewport 反算源桌面像素。操作模式的鼠标和滚轮使用固定 16ms 窗口合并，连续动作不能重置发送期限；文字单独使用固定 200ms 窗口，点击和按键屏障按序立即提交。此规则取代 Alpha 17 的全部输入静默等待。切回只看、失焦、后台或串流销毁立即取消待发并释放已按状态。原生绘制、手势及采样不改变媒体协议。

流畅与响应是串流验收指标：保持源桌面分辨率，外网预算优先调整压缩码率；Android 优先硬件解码，临时输入缓冲压力不得立即丢参考帧，允许丢弃已经解码的陈旧显示帧以追上实时画面。接收、提交、显示帧率和排队 P95 等指标只采集有界数字，不采集画面、键盘文字或账号信息。

远程桌面声音按 [RFC 0016](docs/rfcs/0016-desktop-audio.md) 走专用媒体通道：Rust 管传输与格式，Android 原生解码 Opus 并用 AudioTrack 播放。音频有独立工作线程和有界缓冲，不阻塞视频，不记录声音内容；静音、音频焦点和设备生命周期属于原生播放设置。声音与观看/操作模式独立，关闭串流释放音频资源。

客户端允许同时连接多台主机。Rust `ClientCore` 持有按主机 ID 隔离的工作区，连接、重试、聊天同步、资源和通知轮询独立运行，存储与设备密钥共享且串行写入。主机切换改变选中工作区并关闭原桌面媒体，保留原主机连接及会话选中状态；显式断开和配置修改仅取消目标主机，系统“断开全部”清除全部连接意图。会话列表命令携带主机 ID 与会话 ID，异步执行不得随当前选中项改变目标。排序按主机持久化，默认按创建时间从新到旧，另有从旧到新、最近活动和标题；以会话 ID 稳定处理时间相同的情况，排序不改变选中会话。归档或删除选中项后按操作前顺序选下一条可用会话，末尾选前一条；操作其他项不改变选中。

客户端连接恢复仍由 Rust Core 管理：显式连接持久化全部目标，临时网络/SSH 故障进入可取消的自动重连，以 1/2/4/8/16/30 秒退避；无网络时等待，网络切换或前台恢复唤醒有界只读探测。手动断开和配置修改取消目标主机旧代任务；认证拒绝、未知或变化的主机指纹不自动通过。恢复仅重新连接并观察原会话，不自动创建、恢复 owner 或重放消息、审批和桌面输入。Android 提供默认网络事实、持久前台服务与原生恢复状态；服务重建按 Rust 保存的连接意图恢复，显式断开后保持断开。断线立即退出接管，恢复视频只能重新进入只看模式。

新建会话尚未生成 provider 历史文件是等待状态，不得误报同步失败或用空记录覆盖已有缓存。Chat 不自动退回终端原文；终端诊断由用户显式打开。连接、聊天初始化和历史读取分别呈现状态，临时只读失败可以限频重试，不自动重放用户消息、回答、审批或桌面输入。桌面断开和失败必须可恢复，空闲状态不得持续显示连接动画。

通知按 [RFC 0017](docs/rfcs/0017-agent-notifications.md) 管理：Rust 持有事件身份、主机日志、接收游标、去重和目标验证；原生展示系统通知并处理权限/导航生命周期。通知点击只打开对应主机与会话，不自动执行审批或消息。后台连接通知与进程回收后的系统推送必须分别验证，不能混作同一完成状态。

桌面产品按 [RFC 0015](docs/rfcs/0015-desktop-client-and-host.md) 同时承担客户端和主机角色，覆盖 Linux 与 macOS：客户端连接其他机器，使用目标的会话、agent 和远程桌面；本机节点继续向手机或其他桌面客户端提供服务。主窗口提供远程机器和工作区，本机服务管理独立。客户端复用 Rust Core 与 FFI，媒体仍走独立原生通道；纯客户端不要求本机运行 Herdr/Sunshine。关闭客户端或断开外连不得停止本机后台服务。Alpha 23 仅完成 Linux 主机阶段，桌面客户端须单独通过跨机聊天、视频和输入验收。

Linux Node 的兼容升级按 [RFC 0018](docs/rfcs/0018-live-node-upgrade.md) 采用 systemd 持有 listener、新代接收新连接、旧代自然排空的交接方式；适配器按源码摘要固定。通知/推送单写者排空后才启用新代观察任务，不重放用户命令；GUI 更新不得强制关闭正在使用的面板。首次旧版迁移与普通进程重启不能声明为无缝升级。

## 2. 仓库布局

Docker 部署按 [RFC 0028](docs/rfcs/0028-docker-linux-host.md) 作为用户级 Linux 安装之外的另一条交付路径：镜像为纯服务端，只安装 `kite-node`、Herdr、Sunshine、Bun 及用于 Sunshine 采集的 systemd、Xorg/Xfce、音频、输入法和中文字体；不构建或安装 `kite-desktop`、Kite 面板、托盘或客户端。容器外的客户端继续使用同一 SSH/Node/媒体接口和真实 cgroup v2 agent 预算。每台专用 Linux 服务器只运行一个完整桌面实例；镜像不包含用户凭据，home 与 SSH 身份分别持久化。容器替换是维护重启，不声明为 Node 的原位无缝交接。容器运行脚本位于 `deploy/docker`，部署命令属于 `@kite/linux-host`。

主机资源与进程预算按 [RFC 0022](docs/rfcs/0022-host-resources-and-agent-budget.md)：Linux 适配器提供有界事实并执行 cgroup 机制，Rust 管理 CPU 差分、内存预警、共享预算和启动预留；客户端主机详情只展示与配置。资源同步独立于聊天。所有受管 provider 及本地工具子进程的实际内存计入共享预算；可回收的未映射文件缓存和内核缓存独立展示，不计入面板用量、预警、启动准入或调低校验，控制服务不进入 agent 预算。内核硬上限保持不高于预算，随常驻分配回收缓存，不因缓存放宽实际内存上限。已有进程迁移保守预留旧内存账目；严重压力或未落实限额时拒绝新启动，不自动重放任务。限额修改原子持久化，降低至当前实际用量以下时拒绝，安全调低先回收缓存并核验总量。

Agent 能力按 [RFC 0019](docs/rfcs/0019-agent-docs-and-linux-desktop.md) 经统一 `kite docs`、CLI 和标准 MCP 暴露。文档正文仅维护一份，薄 SKILL 负责发现，按主题及调用方显式 revision 渐进返回；不得全局假定客户端已读。Linux X11 动作由应用层验证/选择目标，Bun/TypeScript 平台适配器实现桌面机制；截图用私有制品和标准图像块，不以 base64 文本消耗模型上下文。新桌面接口允许任务授权范围内的输入与 argv 启动，不自动通过 shell 解释命令；既有只读 MCP 合同保留。

```
SPEC.md                  本规范
Cargo.toml, Cargo.lock   Rust workspace 根，成员是 crates/*
package.json, bun.lock   Bun workspace 根，成员是 packages/*
packages/<package>/     TypeScript 服务端协议、机制、provider 适配和 Linux 交付工具包
crates/<crate>/          每个 crate 一个目录，目录名就是 crate 名
desktop/linux/           Linux 原生主机控制面板与用户级安装入口
target/                  构建输出，不提交
android/                 Android 应用（Gradle 工程）
ios/                     iOS 应用（XcodeGen 工程，描述文件是 ios/project.yml）
macos/                   macOS 原生客户端（XcodeGen 工程，描述文件是 macos/project.yml）
docs/rfcs/               设计提案（RFC），见 docs/rfcs/README.md
```

- TypeScript 包使用 `src/` 存放产品或工具实现，`test/` 存放单测、夹具和验收程序；产品源码不得导入测试代码。`@kite/linux-host` 负责 Bun 安装器和打包器，Linux 安装、打包、验收不依赖 Python；原生 Linux 窗口继续由 Rust/GTK 实现。

- 构建生成物一律不提交：`target/`、`android/app/build/`、`ios/build/`、`ios/Generated/`（UniFFI 生成的 Swift 绑定），以及 XcodeGen 生成的 `ios/Kite.xcodeproj`。

- Rust 代码只放在 `crates/` 下，不得再使用 `core/` 目录。
- crate 名用 `kite-` 前缀加小写、短横线分隔。唯一的例外是工具 crate `uniffi-bindgen`。
- 第三方依赖的版本统一写在根 `Cargo.toml` 的 `[workspace.dependencies]` 里，各 crate 用 `xxx.workspace = true` 引用。

## 3. crate 划分

### 3.1 分层

```
原生应用（Kotlin / Swift）
        │  UniFFI · 媒体通道
边界层   kite-ffi        唯一对外的库：Android 用 cdylib，iOS 用 staticlib
        │
应用层   kite-app
        │
领域层   kite-herdr   kite-agent   kite-stream
        │
基础层   kite-ssh     kite-store
```

### 3.2 各 crate 的职责

| crate | 层 | 职责 | 状态 |
|---|---|---|---|
| `kite-ssh` | 基础 | SSH 连接；ed25519 密钥的生成和解析；严格的主机密钥校验；执行命令；端口转发；保活和重连 | 已有连接、密钥、校验、exec/direct-tcpip 通道及保活；重连命令由应用层编排 |
| `kite-store` | 基础 | 本地持久化：主机列表、已信任的主机密钥、对话缓存、设置、本机订阅状态（RFC 0027）。数据目录由原生层在初始化时传入 | 已有主机、信任记录、配对公钥和原子迁移；对话缓存待做 |
| `kite-herdr` | 领域 | 查找 herdr 并检查桥接；JSON API 客户端（agent 列表、事件订阅、发消息、读屏、发按键）；客户端协议的编解码和每个 pane 的单元格模型 | 已有查找、检查和 JSON API；二进制客户端协议待做 |
| `kite-agent` | 领域 | 定位、增量读取和解析对话记录，转成统一的 Item/Block 模型；Thread、引用与分叉来源（RFC 0027）；根据 herdr 状态和对话记录推导状态行（结构化数据，见 4.1） | 已实现（Android Alpha 2）；RFC 0027 实施中 |
| `kite-stream` | 领域 | Sunshine 配对和管理；通过 moonlight-common-c 建立 Moonlight 会话；把视频帧交给 `MediaSink`，接收串流输入 | Android 已接入配对、H.264 和基础输入；音频/iOS 视频待做 |
| `kite-app` | 应用 | 编排领域 crate：多主机运行时、连接生命周期、视图模型（带版本号的快照）、命令；持有 tokio 运行时 | 已有持久会话、快照、任务取消和原生 SecretStore 回调 |
| `kite-ffi` | 边界 | 唯一对外的库（Android 用 cdylib，iOS 用 staticlib）：UniFFI 导出（类型映射、错误映射）；媒体通道（Android 用 JNI，iOS 用 C ABI）；平台日志初始化。**不含业务逻辑** | 已有 Core/observer/SecretStore 导出及 Android JNI 媒体通道 |
| `uniffi-bindgen` | 工具 | 从编译好的库生成 Kotlin / Swift 绑定 | M1 已有 |
| `kite-node` | 可执行入口 | Linux 主机节点 CLI；调用应用层主机运行时，桌面面板通过结构化 CLI 使用它 | RFC 0014 |

### 3.3 依赖规则

允许的内部依赖：

| crate | 只能依赖 |
|---|---|
| `kite-ssh` | 无 |
| `kite-store` | 无 |
| `kite-herdr` | `kite-ssh` |
| `kite-agent` | `kite-ssh`、`kite-herdr` |
| `kite-stream` | `kite-ssh` |
| `kite-app` | 以上全部 |
| `kite-ffi` | `kite-app`（只能是它） |
| `kite-node` | `kite-app`（只能是它） |
| `kite-desktop` | 无内部依赖；原生 GTK 展示，仅调用结构化 CLI |

- 依赖只能从上往下。表里没列出的内部依赖一律不允许，同层之间也一样。
- `kite-ffi` 只依赖 `kite-app`。它需要的领域类型，由 `kite-app` 用 `pub use` 重新导出。
- 领域层和基础层不得依赖 `uniffi`、`jni`、`android_logger` 或任何平台 SDK 的绑定。
- 只有 `kite-ffi` 可以使用 `#[uniffi::export]`、`uniffi::Record` 这类 UniFFI 宏。

Linux 主机端按 [RFC 0014](docs/rfcs/0014-linux-host-node.md) 使用 `kite-app::host_node` 可选运行时，统一私有 IPC、服务策略、主机适配器和 MCP；新 CLI 只处理参数和退出码。Linux GTK 面板属于原生展示层，只调用结构化 CLI，不实现服务领域逻辑。 常驻托盘与窗口共用同一状态轮询器；节点给出结构化服务健康状态，原生负责图标、文案、单实例窗口和登录自启动。关闭面板或退出托盘不得停止后台服务。节点只复用或启动明确配置的服务，不停止外部拥有的 provider、Herdr 或 Sunshine。
- 只有 `kite-ffi` 可以写 `cfg(target_os = "android")` 或 `cfg(target_os = "ios")` 分支。

### 3.4 crate 内部约定

- **错误**：每个 crate 用 thiserror 定义自己的错误枚举。公开 API 不得用 `anyhow` 作为错误类型。上一层用 `#[from]` 包装或显式转换。
- **异步**：统一使用 tokio。只有 `kite-app` 可以创建运行时，而且整个进程只有一个多线程运行时。其他 crate 只提供 async 函数；需要后台任务时可以在调用方的运行时里 `tokio::spawn`，但必须把 `JoinHandle` 或取消手段交还给调用方，不得留下无人管理的任务。
- **超时**：所有网络操作必须有超时。
- **日志**：使用 `log` 宏。不得记录私钥、口令和对话内容；主机地址和密钥指纹可以记录。info 级别只记录状态变化。
- **文字**：Rust 里不得出现给用户看的文案。`Display` 实现只用于日志和诊断，用英文写。
- **测试**：基础层和领域层的测试不得依赖外网或真实主机。需要 SSH 的测试用进程内服务端，参照 [crates/kite-herdr/tests/ssh_check.rs](crates/kite-herdr/tests/ssh_check.rs)。

## 4. Rust 与原生界面的分界

### 4.1 谁负责什么

| 归 Rust | 归原生 |
|---|---|
| 网络和协议：SSH、herdr 的 JSON API 和客户端协议、Moonlight 协议 | 平台生命周期：Activity / Scene、前台服务、画中画、后台策略 |
| 对话记录的解析和统一消息模型 | 渲染：Compose / SwiftUI、终端单元格的绘制、Markdown 和 diff 的展示 |
| 领域状态：主机、连接、agent、聊天消息、状态行 | 布局、导航、手势、动画 |
| 本地持久化：主机、已信任的主机密钥、缓存 | 输入采集：触控、键盘、输入法、指针捕获 |
| 密钥生成和 SSH 认证 | 文案和本地化 |
| 串流协议：配对、会话、组帧，输出视频帧 | 硬件解码和显示：MediaCodec / VideoToolbox、Surface |
| 重连、超时和重试策略 | 敏感数据的安全存储：Android Keystore / iOS Keychain |
| 会话关系：分叉来源、Thread、叠加引用、快照生成；本机阅读状态 | 系统通知、手机剪贴板、权限申请、分享 |
| | 多选、拖拽、嵌入卡片与"引用不可用"状态的展示 |

容易划错的几处：

- **状态行**：Rust 决定显示哪一条、什么时候换（优先级、每秒最多刷新一次、每句至少停留 1.5 秒），输出结构化的 `ActivityLine`，例如"blocked + 提示内容""工具调用 + 动作 + 目标""回复首句"。原生按模板把它写成一句话，并决定颜色和样式。
- **设备私钥**：Rust 负责生成和使用；原生负责安全存储（通过 `SecretStore` 回调，见 4.3）。
- **剪贴板同步**：主机那一侧（经 SSH 读写）归 Rust；手机那一侧归原生。
- **接管时的输入**：原生采集输入，并转换成 kite 统一的输入事件；Rust 负责编码和发送。

### 4.2 三条通道

| 通道 | 方向 | 用途 | 机制 |
|---|---|---|---|
| 命令 | 原生 → Rust | 用户操作：连接、发消息、确认、接管、配对 | UniFFI 导出的方法，耗时的都是 async |
| 状态 | Rust → 原生 | 视图模型快照和事件 | UniFFI 回调接口，由原生实现 |
| 媒体 | 双向 | 视频帧（Rust → 原生）、串流输入（原生 → Rust） | Android 用 JNI 加 direct ByteBuffer，iOS 用 C ABI |

### 4.3 UniFFI 接口约定

- **入口对象**：原生在进程启动时创建唯一的 `KiteCore`（`uniffi::Object`），传入数据目录、缓存目录、设备信息和 `SecretStore` 实现。整个进程共用这一个对象，不随界面重建。
- **类型选择**：生命周期长的东西用 Object（例如主机会话），数据用 Record，状态用 Enum。
- **命令**：方法名用动词开头。耗时操作必须是 async；同步方法不得做 I/O。
- **状态推送**：
  - `kite-app` 维护各个视图模型（主机列表、agent 列表、某个 agent 的聊天、状态行、串流状态等），每个视图模型带一个单调递增的 revision。
  - `kite-ffi` 通过原生实现的 observer 回调推送快照。聊天消息这类不断增长的数据推送增量：追加一条，或更新某一条。
  - 聊天条目以类型化的 Item/Block 记录推送；原生按块顺序展示，附件、工具调用、问答回复和嵌入引用不能互相覆盖或隐藏。只有纯工具 Item 进入折叠工具组。provider 信封必须拒绝不能无损表达的发送组合（RFC 0027）。
  - `conversation_revision` 是进程内展示增量版本，不得持久化为阅读游标；阅读状态使用稳定的 provider Item ID、内容摘要及已验证通知位置（RFC 0027 阶段 5）。
  - 原生只保留最新快照，revision 比手上旧的直接丢弃。
- **回调线程**：回调在 Rust 的工作线程上执行。原生实现必须立即返回：不得阻塞，也不得在回调里同步调回 Rust。需要更新界面时，切到主线程去做。
- **SecretStore**：由原生实现的回调接口，按名字读写字节，底层用 Keystore / Keychain 加密。Rust 需要私钥时向它读取。私钥明文不得写进 `kite-store`。
- **错误**：
  - 每个导出函数返回 `Result<T, KiteError>`。
  - `KiteError` 的每个变体是一个稳定的错误码，只带结构化字段。原生按变体选择文案。
  - `details` 字段只是诊断信息，不得当作主要提示直接展示，只能放在"详情"一类的次要位置。不得用 `message` 命名导出错误的字段，以免与 Kotlin 的 `Throwable.message` 冲突。
- **panic**：release 构建保持 `panic = "unwind"`，由 UniFFI 转成原生异常；原生显示通用错误并记录日志。Rust 代码不得用 panic 做正常的流程控制。
- **数据类型**：
  - ID 用不透明的字符串。
  - 时间用 Unix 毫秒（`i64`）。
  - 字节数组只用于小数据（64 KB 以内）。
  - 不得跨边界传递裸指针。
- **版本**：绑定在每次构建时从编译好的库生成，不提交进仓库。Rust 和原生总是同一次构建的产物，所以 FFI 接口不需要向后兼容。herdr 协议则不同，必须兼容。

### 4.4 媒体通道约定

- `kite-stream` 只依赖抽象的 `MediaSink` trait（接收视频帧和音频帧）和 kite 自己的输入事件类型，不知道 JNI 的存在。
- `kite-ffi` 在 `android` 模块里用 jni crate 实现 `MediaSink`：把帧写进原生分配的 direct ByteBuffer，再调用 Kotlin 的解码器。在 `apple` 模块里用 C 回调实现。
- Surface、MediaCodec 和解码器配置都归原生，Rust 不持有 Surface。
- 同一路串流的所有输入事件必须走同一条通道，保证顺序。高频的指针移动可以在原生侧先合并再发送。
- 帧回调里不得分配大对象，也不得加锁等待界面线程。

## 5. 原生层约定

### 5.1 两个平台共同的规则

- **ViewModel 只做三件事**：把 Rust 推来的快照转成界面状态；把用户操作转成 Rust 命令；选择文案。不得在 ViewModel 里实现网络、协议或业务规则。
- 不得修改 Rust 推来的快照对象。
- UniFFI 生成的绑定不得手写或手改。
- 文案只写在平台的字符串资源里，代码里用语义化的 key。

### 5.2 Android

- **包结构**：
  - `io.github.wenshiqi0.kite.core`：生成的绑定。
  - `io.github.wenshiqi0.kite.ui`：Compose 界面和 ViewModel。
  - `io.github.wenshiqi0.kite.data`：平台存储的适配，例如 `SecretStore` 的实现。
  - `io.github.wenshiqi0.kite.media`：解码和显示（M3）。
- **文案**写在 `strings.xml` 里。
- **敏感数据**用 Android Keystore 中的密钥加密后保存。

### 5.3 iOS

- **工程**：用 XcodeGen 维护 `ios/project.yml`，不得手改生成的 `.xcodeproj`。最低支持 iOS 17，一个通用 App 同时覆盖 iPhone 和 iPad。
- **Rust 接入**：Xcode 的编译前脚本 `ios/scripts/build-rust.sh` 为当前平台编译 `kite-ffi` 静态库，并把 Swift 绑定生成到 `ios/Generated/`。
- **目录结构**：
  - `ios/Kite/App`：入口。
  - `ios/Kite/UI`：SwiftUI 视图和 ViewModel（`@Observable`）。
  - `ios/Kite/Data`：平台存储的适配，例如 Keychain。
  - `ios/Kite/Resources`：`Localizable.xcstrings` 和资源目录。
- **文案**写在 `Localizable.xcstrings` 里。
- **敏感数据**存进 Keychain：只在本机可用（`ThisDeviceOnly`），不同步到 iCloud。
- **ViewModel 更新界面**：只能在主线程（`@MainActor`）上修改界面状态。

### 5.4 iPhone 与 iPad

- **布局必须由尺寸类别（size class）和可用宽度决定，不得由设备型号决定。** iPad 在分屏、侧拉或者台前调度的小窗口里是紧凑宽度，这时必须使用和 iPhone 一样的布局。
- 设备型号只能用来判断能力，例如是否支持多窗口。硬件键盘和指针在 iPhone 上也可能连接，所以按"是否连接"判断，不按设备型号判断。
- 同一功能在紧凑宽度和常规宽度下的差异，写在对应功能的 RFC 里。总体原则见 [RFC 0002](docs/rfcs/0002-ios.md)。

### 5.5 macOS

按 [RFC 0027](docs/rfcs/0027-macos-client.md) 实现 macOS 14+ 原生客户端，按 [RFC 0029](docs/rfcs/0029-macos-host.md) 集成本机主机角色。SwiftUI/AppKit 面板通过随应用交付的结构化 Node CLI 管理 launchd 用户服务；关闭客户端不停止后台节点或 agent。Mac 沿用 SSH、私有 IPC 和既有会话协议；进程机制使用 Darwin API。Mac 不声明 Linux 内存硬限额、进程暂停或 X11 桌面 MCP 能力。客户端有独立本机 Keychain 身份和应用支持目录，不迁移其他设备私钥。XcodeGen 维护 `macos/project.yml`，生成的工程、绑定和构建目录不提交。界面更新使用 MainActor，文案使用平台字符串资源。

Apple 媒体在 FFI 的 `apple` 模块使用 C ABI 回调和不透明整数 handle，注册/在途串流持有上下文，最后一个所有者释放时执行原生 release。H.264 由 VideoToolbox 解码、Metal 展示，音频原生播放；高频输入不走 UniFFI，Rust 再次检查操作状态。失焦或目标退出释放输入并取消待发事件，异步命令与剪贴板始终绑定明确的目标上下文。客户端退出只释放外连资源，不管理本机主机服务生命周期。

Mac 本机开发版按 [RFC 0028](docs/rfcs/0028-macos-keychain-access.md) 固定应用签名，使用只安装一次、按需启动的原生钥匙串组件保持读取身份。组件只接受指定证书和 Kite bundle ID 的父进程，只处理两个既有条目，秘密仅通过私有管道传递。普通读写不弹授权 UI，禁止悄悄回退或重新生成身份；显式设置更新访问控制并保留原数据，正常构建不覆盖组件。

## 6. 当前实现与 M1 升级

RFC 0003 已落实持久 Core、主机存储和 Android 远程桌面。原生 C 适配层隐藏 Moonlight ABI，单进程仅一路串流。FFI 库和绑定同次生成，不保留旧 FFI 入口；herdr 0.9.5 协议必须兼容。RFC 0003 阶段不含结构化聊天；RFC 0004 已落实 Android 聊天，iOS 视频仍待实现。

| M1 现状 | 当前升级处理 |
|---|---|
| SharedPreferences / UserDefaults 保存主机与指纹 | 仅作只读迁移源；Rust 原子写入主机、所有公开指纹和迁移标记，重试幂等，冲突不覆盖 |
| 原生读取私钥并作为函数参数传入 | `SecretStore` 回调复用原有 Android wrapping key/密文文件或 iOS Keychain 项；不重建设备身份 |
| 顶层一次性检查 | 进程唯一 `KiteCore`，唯一应用运行时，连接、API 轮询和桌面任务持有取消手段与句柄 |
| 检查后关闭 SSH | 保持 SSH，桌面控制通过 direct-tcpip；视频与实时输入通过私有网络 Moonlight |
| ViewModel 自组领域状态 | Rust 推送带 revision 的快照，原生丢弃旧版本，仅保留展示和输入状态 |

Android 使用 MediaCodec 与 SurfaceView，编码帧通过预分配 direct ByteBuffer 传递，不走 UniFFI。默认只看，进入操作模式才转发输入；切回只看释放按键。后台在系统 PiP 中保留视频，否则暂停；前台服务管理 SSH 生命周期，并处理快速连接失败时先提升前台、再停止服务的顺序。

验收必须分别确认真实 herdr API、持续视频接收、实际解码呈现和输入到达。编译、`/serverinfo` 与编码包计数不能代替用户可见的画面和操作证据。

## 7. 修改本规范

- 新增 crate、改变依赖规则、改变 Rust 与原生的分界或通道机制，必须先在 [docs/rfcs/](docs/rfcs/README.md) 写一份 RFC。RFC 被采纳后，在同一个改动里更新本规范，然后再改代码。
- 措辞修正、补充例子这类不改变规则的修改，可以直接改本规范，不需要 RFC。
- 本规范和 RFC 冲突时，以本规范为准。已采纳的 RFC 如果没有同步进本规范，视为还没生效。
- 代码评审以本规范为准。和规范冲突的代码，要么改代码，要么走 RFC 流程修改规范。

## 8. Bun 服务端 packages

跨机器工作区与网络 ACP 按 [RFC 0030](docs/rfcs/0030-network-acp-and-shared-workspaces.md) 实施：Core 发布全部主机的会话目录，原生侧栏以机器分组，搜索与新建为全局入口；打开会话同时绑定目标机器和会话。网络 ACP 使用现有 owner、工作目录和资源预算，首阶段为已鉴权的 WebSocket `/acp`，不等同于完整 HTTP/2 Streamable HTTP。网络连接断开不能终止或重放用户工作，跨机器通过已信任 SSH 建立通道，provider 密钥保留在执行主机。

按 [RFC 0024](docs/rfcs/0024-bun-server-monorepo.md)，仓库同时使用 Cargo workspace (`crates/*`) 与 Bun workspace (`packages/*`)。服务端平台及 provider 适配器使用 TypeScript strict 校验与 Bun 运行时，取代原 `backend/` Python 运行路径；原生 GTK 面板、安装与发布工具不属于服务端 TS 运行边界。Rust 继续持有客户端领域状态、SSH/IPC 与主机策略。packages 依赖从 host 到 providers/platform、runtime、protocol 单向流动。所有外部 JSON 必须从 unknown 验证；不得通过 any、关闭 strict 或忽略类型错误绕过检查。构建 bundle 进入 Cargo OUT_DIR，按摘要释放；生成物不提交。

客户端内容必须使用原生语言：Android Kotlin，iOS/macOS Swift，Linux Rust/GTK。`kite-desktop` 为原生展示可执行入口，使用结构化 CLI 调用 `kite-node`；不得将 UI 迁入 Bun/TypeScript，也不得保留 Python GUI。该 crate 不引入 provider 或服务策略。
