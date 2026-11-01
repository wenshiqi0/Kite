# Harness 接入：客户端合同与展示

> 仅在实现、修改或排查 provider 的客户端接线、能力和展示时读取。先看[接入顺序](harness-integration.md#完整接入的实施顺序)；主机机制按需读[服务端步骤](harness-server.md)。

## 1. 当前缺口

当前 Android 已从主机验证的 `CoreSnapshot.providers` 生成新建选项，并按操作能力控制初始化、发送、模型、附件与 Skill 入口。名称识别包括 Codex、Claude 和 DSH；未知保存记录保留原 ID。它仍不代表任意 harness 都已有协议适配。

目录和布尔能力已实现；下文还包括后续扩展时需要逐项处理的完整要求，不把尚未实现的能力当作现有 API。DSH 的本阶段范围见[配置与验证](dsh.md)。

## 2. 先补齐 Rust 到客户端的数据合同

复用 `kite-agent::providers::Provider` 与 FFI `ProviderInfo` 的可用目录和操作元数据。静态支持信息由 Rust provider 定义维护；安装、版本、认证/连接等事实由目标主机探测；Core 汇合并发布，原生负责名称资源、图标、文案和控件。不得分别在 Kotlin、Swift 和主机脚本维护三套“支持哪些 harness”的业务规则。

| 数据 | 必需内容 | 作用域与发布位置 |
| --- | --- | --- |
| provider 目录项 | 稳定 `id`、品牌/资源标识、实际可用性、结构化不可用原因、已实现能力 | 当前目标主机；加入 Core `Snapshot`，经 FFI `CoreSnapshot` 发布。仅返回经验证可用的 provider；用户未接入的未知 ID 不伪装成已支持 |
| 会话身份 | Kite session ID、provider ID、原始引用、活动 agent ID、逻辑归档状态 | 保留现有 `ConversationSession` / `SessionInfo`，所有操作与晚到更新都核对目标身份 |
| 会话就绪 | 初始化中、就绪、失败、不可用/离线，及失败原因和可重试操作 | 当前会话；文本可提交与模型/额度加载分别表示，不能只有一个“Codex composer ready”条件 |
| 可用操作 | 创建、恢复、文本提交、模型选择、思考强度、图片/文件/音频、Skill、额度、审批/问题、turn 取消与进程暂停 | 先判定是否支持，再结合当前连接/会话/模型判断是否可操作；不支持、未知、暂时不可用分别表达 |
| 消息与交互 | 现有消息/工具模型、`HistoryInfo`、`ResponsePhase`、`ApprovalInfo` / `QuestionInfo` | 复用统一模型与会话范围；不向原生透传需要其理解的厂商事件 |

当前使用 `Provider` / `ProviderInfo`；进一步扩展细分能力时同步领域与 FFI，不在文档示例中发明已经可调用的接口。能力默认未知或不支持，不能因 provider 字符串非空就全部开启。

三个判断必须独立：

1. **已实现**：Kite 的该 provider 适配器是否支持这个操作。
2. **当前可用**：所选主机/版本、会话和模型是否满足条件。
3. **本次可操作**：连接、初始化、归档、暂停或阻塞交互等状态是否允许执行。

例如 `acme` 只支持文本、历史、工具和模型选择时：文本就绪即可发送；模型菜单可用，思考强度、附件、Skill、额度入口按不支持处理。它无需等待 Codex 的模型/额度接口成功。换主机或模型后重新计算可用操作，不能沿用前一个会话的数据。

## 3. 页面与控件展示

| 页面/入口 | 展示与操作要求 |
| --- | --- |
| 新建会话 | 从当前主机的 provider 目录生成选项，展示正确名称（和已提供的品牌资源）；仅展示后端验证可用项，关闭开关、未安装或探测失败的 provider 不出现。确认时提交稳定 ID、cwd 和标题；目录加载中、失败、空列表分别呈现。不得缺省把未知 ID 当 Codex |
| 默认选择 | 保留已有缺省；用户显式选过的 provider 在本次创建中保持稳定。不可用时说明原因，不静默换另一个 harness 创建会话 |
| 会话抽屉 | 每行显示 provider、会话标题/项目、预览和状态。未知 provider 使用保存的 ID/中性图标，缓存历史仍可读；品牌资源缺失不显示成 Codex，也不丢记录 |
| 顶栏与项目标题 | provider 品牌与项目名称分别来自明确元数据；不能只用 `kite-(codex|claude)-...` 正则剥离内部名字。长名称、省略和辅助功能标签仍可辨认当前目标 |
| 初始化/失败 | 创建与连接中显示同一初始化提示并锁住发送；失败显示明确重试入口。新会话未生成历史不算同步失败，不展示前一个会话的内容；只读重试不重复提交用户消息 |
| 输入框与发送 | 按文本支持及本会话就绪启用；离线、归档、未连接、暂停和阻塞交互给出相应原因。无需模型选择能力也能纯文本聊天。普通失败保留草稿，发送结果未知时不自动重发 |
| 模型与思考强度 | 两个独立能力。支持模型选择才显示真实目录；思考强度列表由当前模型事实提供，无该能力就不显示。加载失败只影响相应设置，不把仍可用的文本聊天变成不可用 |
| 附件与 Skill | 图片、文件、音频和 Skill 分别按能力及模型限制显示；不要整体绑定 `provider == codex`。已有不兼容草稿保留并明确提示处理，不能切会话后悄悄发送或静默丢弃 |
| 聊天与工具过程 | 复用统一用户消息、公开进度、最终回复和工具组件；公开文本直接显示，仅相邻工具合并折叠。保留稳定 ID、失败详情、完整命令原文和工件入口；不为每个 provider 复制一个时间线 |
| 审批与模型问题 | 按真实请求显示已有卡片；响应关联当前会话/request。异步问题提供注意提示但不自动锁住普通聊天；阻塞问题与审批按实际等待语义处理。不支持时只提供已验证的真实终端入口 |
| 暂停/继续与取消 | 暂停进程、恢复进程、取消 turn 分别使用已验证操作。抽屉、顶栏与全屏工具栏使用同一能力状态，不把未知 provider 默认允许暂停，也不把暂停用作桌面接管的副作用 |
| 额度 | 支持且有数据时展示该 provider 的真实用量/窗口；未提供、失败或过期分别表示。不要给 `acme` 显示“请连接 Codex”或拿 Codex bucket 当它的额度 |
| 断线与恢复 | 缓存历史仍可阅读，写操作禁用；恢复相同逻辑会话后重新探测状态和交互。显式恢复不改变 provider，归档不结束后台 agent |
| 通知与回访 | 展示统一完成/待处理事实；点击只打开确切主机/会话。未知 provider 可查看已保存事件，不能因名称识别失败跳到其他会话或自动批准 |

暂时不可用的已支持能力保留合理的加载/重试反馈；从未支持的能力不放一个永久 loading 控件。主机/会话的调试细节放在按需打开的诊断入口，不占日常聊天流程。

## 4. Android 逐文件实施表

以下是当前源码中的具体改动面，路径均相对于仓库根。新增 provider 前按对应符号检查，避免只修改新建会话的两个按钮。

| 文件/符号 | 当前行为 | 接入时要做的事 |
| --- | --- | --- |
| [core.rs / Snapshot](../../crates/kite-app/src/core.rs) 与 [core/sessions.rs](../../crates/kite-app/src/core/sessions.rs) | provider 创建校验和 Codex 专用激活/composer 分支 | 发布 §2 的目录与会话状态，让同一份能力驱动命令校验与客户端操作 |
| [core_bridge.rs](../../crates/kite-ffi/src/core_bridge.rs) 的 `CoreSnapshot` / `AgentInfo` / `SessionInfo` / `ComposerInfo` | 已有 `providers` 目录；`can_pause` 映射领域操作元数据 | 映射共享数据，将业务判断移回 Core/领域；新增字段与原生生成绑定一起交付 |
| [KiteWorkspace.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/KiteWorkspace.kt) 的新建对话框 | 现已按 `providers` 列表生成选项，保留默认 Codex 与 provider 提示 | 用目录项呈现选择，补品牌资源和不可用原因；提交 ID，保留原有 cwd/标题输入和防重复创建 |
| [SessionDrawer.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/SessionDrawer.kt) 的 `providerLabel` / `sessionTitle` | 现已识别 DSH，未知 ID 原样回退；历史预览仍有 Claude 特判 | 正确解析每个品牌并保留未知 ID 回退；无活动进程/草稿状态按通用可用性展示 |
| [KiteWorkspace.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/KiteWorkspace.kt) 的 `agentTitle`、恢复提示、`composeReason` | 内部标题正则只识别两家；恢复文案及未就绪判断有特判 | 使用明确元数据与可用操作生成文案，处理 provider 未安装、待连接、初始化、失败、归档等情况 |
| [SessionInitialization.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/SessionInitialization.kt)、[HostCheckViewModel.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/HostCheckViewModel.kt) 的 `sessionStartup` / `submissionLocked` | 只有 Codex 等待 composer.ready；其他 provider 很容易过早变为 READY | 展示并遵循 Core 的会话就绪结果；首次激活与切换不能复用旧 ready。ViewModel 只负责平台草稿与展示，不重新推导协议就绪 |
| [ComposerControls.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/ComposerControls.kt) / `KiteWorkspace` 加号菜单 | 模型/附件/Skill 入口有 Codex 判断 | 按独立能力展示模型、思考强度、图片、文件、音频和 Skill；保留上传/目录的空、加载、失败、部分失败与重试 |
| [AccountUsage.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/AccountUsage.kt) | Codex bucket、Claude 特定提示、刷新仅认 Codex | 消费对应 provider 的额度事实；不支持/未知/离线使用通用本地化说明，刷新能力单独校验 |
| [ChatTimeline.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/ChatTimeline.kt)、[ToolGroupCard.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/ToolGroupCard.kt)、`KiteWorkspace.MessageCard` | 已有通用消息与工具虚拟展示 | 优先直接复用；只扩展统一模型所需的内容类型与品牌无关展示，不加入 provider 日志解析 |
| [ModelQuestionCard.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/ModelQuestionCard.kt)、`KiteWorkspace.ApprovalCard`、[ResponsePresentation.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/ResponsePresentation.kt) | 已有问题、审批和注意/响应阶段组件 | 使用统一 request 信息与阶段，不用普通文本造卡；过期、重复回答、跨会话目标由 Core 拒绝 |
| [FullscreenRail.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/FullscreenRail.kt) 和工作区暂停按钮 | 使用 `AgentInfo.canPause` | 继续消费同一 Core 能力，不在全屏入口再维护 provider 白名单；有取消 turn 时另设明确语义 |
| [CoreController.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/data/CoreController.kt) | 应用级 observer 与时间线增量 | 接受新增快照/绑定；保留按 context/revision 的消息镜像，不因能力/视频更新重建全部消息 |
| [strings.xml](../../android/app/src/main/res/values/strings.xml)、[ProductIcon.kt](../../android/app/src/main/java/io/github/wenshiqi0/kite/ui/ProductIcon.kt) 与需要的品牌资源 | 现有品牌、操作和错误文案 | 补新品牌与通用不可用/连接原因，保持未知 ID 回退、深色主题、长名称和辅助功能标签；不暴露内部协议术语 |

定位硬编码可先用 `rg -n 'codex|claude|providerLabel|can_pause|composerReady' crates/kite-app/src crates/kite-agent/src crates/kite-ffi/src android/app/src/main`，按语义逐项处理；Codex 专用适配器内的协议字符串可以保留，不能批量替换为新名字。

## 5. iOS 与桌面客户端的范围

Rust 与 FFI 合同共享，原生实现各自验收。当前 [iOS CoreController](../../ios/Kite/Data/CoreController.swift) 接收快照，[HostCheckView](../../ios/Kite/UI/HostCheckView.swift) 仍以主机检查为主，不能把 Android 新 harness 聊天完成说成 iOS 已完成同等界面。如果交付范围包含 iOS，必须补 provider 选择、会话/消息、能力控件和对应的 SwiftUI 体验，再用真实会话验收；不在 Swift 中另写一套 provider 协议。

Linux 现有 [desktop.py](../../desktop/linux/desktop.py) 是主机面板，不是远程聊天客户端。Linux/macOS 客户端仍按 [RFC 0015](../rfcs/0015-desktop-client-and-host.md) 的阶段推进；仅安装新主机适配器不算这些客户端的展示已经支持。纯客户端场景使用远程目标的 provider 目录，不要求本机安装该 harness。

## 6. 客户端验收

扩展现有 [SessionInitializationTest](../../android/app/src/test/java/io/github/wenshiqi0/kite/ui/SessionInitializationTest.kt)、[ChatTimelineTest](../../android/app/src/test/java/io/github/wenshiqi0/kite/ui/ChatTimelineTest.kt)、[ResponsePresentationTest](../../android/app/src/test/java/io/github/wenshiqi0/kite/ui/ResponsePresentationTest.kt)，并按接入能力新增目录、标签与控件测试。新增测试的名称由实现决定，不能引用不存在的测试当作已通过。

至少覆盖这些可观察结果：

1. 目录含 Codex、Claude、`acme` 与未知保存记录；未安装项/关闭开关/探测失败项不进入新建目录；选择、显示名、图标均准确，未知记录不显示 Codex。
2. 从已就绪的 Codex 切到新 `acme` 时，先进入初始化且不能发消息；`acme` 纯文本就绪后可发送，即使它没有思考强度、附件或额度能力。失败后有重试，重试不重复提交 prompt。
3. 每个可选控件独立按能力出现，随主机/模型/会话变化更新；不兼容草稿不静默丢失或误投，晚到模型/额度/问题结果不污染当前会话。
4. 用统一消息数据渲染首条回复、公开进度、相邻工具组、工具失败、问答与历史回填；断线缓存、长文本虚拟渲染和滚动位置仍正确。
5. 异步问题不会被当作阻塞问题；回答和审批提交一次、目标准确，过期响应被拒绝；暂停与取消有各自的能力和状态。
6. 用隔离的真实目标 harness 会话完成“新建 → 发第一条 → 看真实回复/工具 → 重连继续 → 归档/恢复”。纯夹具、绑定编译、主机状态或 CLI 成功不能代替这条原生展示验收。

原生控件先用合成快照验证展示，真实模型用自有会话验证接线；两类证据分开记录。已有 [NewSessionRegressionTest](../../android/app/src/androidTest/java/io/github/wenshiqi0/kite/NewSessionRegressionTest.kt) 等 live 用例固定为 Codex 和自有 QA 环境，新增 provider 时扩展自有夹具与守卫，不直接把 provider 参数替换后跑到用户会话。

执行入口遵循[测试说明](../testing.md)。FFI 数据变化必须同次重生成并编译对应原生绑定；需要构建 APK 时，按仓库约定连同源码提交、推送和 Alpha Release 一起完成，而不是只给本地包。仅修改本组规范时检查源码引用、链接和渐进披露范围。
