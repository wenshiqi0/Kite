# kite 工程规范（SPEC）

> 状态：生效 · 2026-10-28
> 适用范围：`crates/` 下的全部 Rust 代码，以及 `android/`（以后还有 `ios/`）下的原生代码。
> 本文汇总当前生效的硬性规则。每条规则的来龙去脉记录在 [docs/rfcs/](docs/rfcs/README.md) 里，整体设计见 [RFC 0001](docs/rfcs/0001-architecture.md)。要调整 crate 划分，或者调整 Rust 与原生界面的分界，按第 7 节的流程先写 RFC、改这份规范，再改代码。

文中"必须""不得"是硬性要求，"应当"是默认做法，有充分理由时可以例外，但要在代码或提交说明里写明理由。

## 1. 总原则

1. **Rust 管"做什么"，原生管"怎么呈现"。** 与平台无关、各平台都需要的逻辑，在 Rust 里只实现一次。
2. **状态单向流动。** Rust 持有领域状态，向原生推送不可变的快照；原生只能通过调用命令改变状态，不得自己另存一份领域状态。
3. **用户看到的文字由原生层负责。** Rust 只交出结构化数据，文案和本地化在原生层。
4. **媒体数据不走 UniFFI。** 视频帧和高频输入走专门的媒体通道（见 4.4）。

聊天工作区以 [2026-10-29 产品稿](docs/product/2026-10-29/README.md) 和 [RFC 0004](docs/rfcs/0004-chat-desktop-workspace.md) 为交互依据：接管与 agent 暂停独立；观看缩放不发送输入；布局和 PiP 由原生层处理，真实会话、审批与暂停由 Rust 处理。

手机操作按 [RFC 0005](docs/rfcs/0005-mobile-operation-and-session-entry.md) 使用统一源桌面坐标系和显式只看/操作模式；连接阶段与会话启动仍由 Rust Core 管理。

聊天模型、思考强度和附件按 [RFC 0006](docs/rfcs/0006-rich-composer.md) 使用 Codex app-server v2：同一 pane 的 TUI 和手机连接同一持久服务器，保留 thread ID。`kite-agent` 持有结构化聊天协议和有界附件传输，原生仅提供私有缓存文件路径和展示。模型目录、设置、上传状态归 Rust；普通文件及不支持原生音频时通过主机路径提供给 agent。Linux 辅助程序位于 `backend/`，不得修改全局 Codex 服务或凭据。

会话、归档、模型提问和响应阶段按 [RFC 0007](docs/rfcs/0007-sessions-and-interactions.md) 管理：Rust 持有持久记录、提供方、选中会话、回答关联和运行阶段；主机会话索引使用原子修订号写入，辅助程序承担单会话恢复租约及 Linux 终端启动。原始 provider 历史不移动，逻辑归档不停止后台任务。Android 负责工作过程折叠、题目输入及阶段动画，Claude 未连接时明确显示待连接状态。

模型 Markdown 的原生富文本展示及视频测量按 [RFC 0008](docs/rfcs/0008-native-markdown-and-video-layout.md) 实现：Android 使用 Markwon 绘制 assistant 回复与公开进度，保存的原文和后端协议不变；视频子平面在 measure/layout 中同步定尺寸，显示与输入继续共用源桌面坐标系。

聊天中的异步问题按 [RFC 0009](docs/rfcs/0009-async-questions-in-chat.md) 处理：Rust 关联真实 call ID、恢复待答表单，并通过同一线程的 TUI 回答信封提交；阻塞问题继续使用其 RPC 响应。提问控制记录通过同步升级的 Codex 历史体 v2 独立传输，主机只索引原始事实，Rust 决定待答状态，避免最近聊天窗口截断旧问题。Android 在聊天和输入区提供回答入口，异步问题不暂停 agent 或禁用普通消息。

Android 混合输入和双向文本剪贴板按 [RFC 0010](docs/rfcs/0010-hybrid-input-and-clipboard.md) 实现：触屏绝对定位与仿真鼠标相对移动并存；鼠标按键、滚轮和本地视口手势分开处理。远端系统剪贴板由 Rust 经 SSH 操作，本机剪贴板归原生；限主机身份和当前操作会话、大小与超时，不记录内容。代码和审批命令保留原文，选项展示样式不改变提交值。

紧凑桌面控件按 [RFC 0011](docs/rfcs/0011-compact-desktop-controls.md) 排列为桌面、可收起触控板、Chat与底部输入框；横屏鼠标附着于整个工作区。面板和键盘由原生互斥管理，保持源坐标映射与单解码器。

长会话按 [RFC 0013](docs/rfcs/0013-complete-transcript-and-dark-workspace.md) 独立同步并保留完整数据。Rust 管理完整消息、持久缓存与同步，状态快照不携带正文，原生通过版本增量维护展示镜像。原生只管理虚拟渲染窗口、滚动和跟随最新，滚动不得触发历史网络请求或淘汰逻辑消息。全屏保留鼠标形状控件与右侧操作栏，触控板只用于上下分屏；界面使用统一深色配色。

全屏鼠标展开后，箭头与左右键控件保持固定的屏幕相对位置，缩放和平移不改变这个偏移；每次操作使用 DesktopViewport 反算源桌面像素。全屏输入在连续动作停止 200ms 后合并提交，位置合并但按键、拖动起止、滚轮和键盘事件保持顺序；切回只看、失焦、后台或串流销毁立即取消待发并释放已按状态。原生绘制、手势及采样不改变媒体协议。

## 2. 仓库布局

```
SPEC.md                  本规范
Cargo.toml, Cargo.lock   Rust workspace 根，成员是 crates/*
crates/<crate>/          每个 crate 一个目录，目录名就是 crate 名
desktop/linux/           Linux 原生主机控制面板与用户级安装入口
target/                  构建输出，不提交
android/                 Android 应用（Gradle 工程）
ios/                     iOS 应用（XcodeGen 工程，描述文件是 ios/project.yml）
docs/rfcs/               设计提案（RFC），见 docs/rfcs/README.md
```

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
| `kite-store` | 基础 | 本地持久化：主机列表、已信任的主机密钥、对话缓存、设置。数据目录由原生层在初始化时传入 | 已有主机、信任记录、配对公钥和原子迁移；对话缓存待做 |
| `kite-herdr` | 领域 | 查找 herdr 并检查桥接；JSON API 客户端（agent 列表、事件订阅、发消息、读屏、发按键）；客户端协议的编解码和每个 pane 的单元格模型 | 已有查找、检查和 JSON API；二进制客户端协议待做 |
| `kite-agent` | 领域 | 定位、增量读取和解析对话记录，转成统一的消息模型；根据 herdr 状态和对话记录推导状态行（结构化数据，见 4.1） | 已实现（Android Alpha 2） |
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

- 依赖只能从上往下。表里没列出的内部依赖一律不允许，同层之间也一样。
- `kite-ffi` 只依赖 `kite-app`。它需要的领域类型，由 `kite-app` 用 `pub use` 重新导出。
- 领域层和基础层不得依赖 `uniffi`、`jni`、`android_logger` 或任何平台 SDK 的绑定。
- 只有 `kite-ffi` 可以使用 `#[uniffi::export]`、`uniffi::Record` 这类 UniFFI 宏。

Linux 主机端按 [RFC 0014](docs/rfcs/0014-linux-host-node.md) 使用 `kite-app::host_node` 可选运行时，统一私有 IPC、服务策略、主机适配器和 MCP；新 CLI 只处理参数和退出码。Linux GTK 面板属于原生展示层，只调用结构化 CLI，不实现服务领域逻辑。节点只复用或启动明确配置的服务，不停止外部拥有的 provider、Herdr 或 Sunshine。
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
| | 系统通知、手机剪贴板、权限申请、分享 |

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
