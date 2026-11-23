# 客户端分界、通道与多端原生规范

> 所属规范：[SPEC.md](../../SPEC.md) · 状态：生效 · 更新：2026-11-23
> 相关提案：[RFC 0001](../../rfcs/0001-architecture.md)、[RFC 0002](../../rfcs/0002-ios.md)、[RFC 0003](../../rfcs/0003-persistent-desktop.md)、[RFC 0004](../../rfcs/0004-chat-desktop-workspace.md)、[RFC 0005](../../rfcs/0005-mobile-operation-and-session-entry.md)、[RFC 0010](../../rfcs/0010-hybrid-input-and-clipboard.md)、[RFC 0011](../../rfcs/0011-compact-desktop-controls.md)、[RFC 0015](../../rfcs/0015-desktop-client-and-host.md)、[RFC 0016](../../rfcs/0016-desktop-audio.md)、[RFC 0027-mac](../../rfcs/0027-macos-client.md)、[RFC 0028-keychain](../../rfcs/0028-macos-keychain-access.md)、[RFC 0031](../../rfcs/0031-windows-client.md)

## 1. Rust 与原生界面的职责边界

| 归 Rust（`crates/*`） | 归原生（Kotlin / Swift / C# / GTK） |
| --- | --- |
| 网络与协议：SSH、Herdr JSON API、Moonlight 串流协议、主机 Node IPC | 平台生命周期：Activity / Scene、前台服务、画中画（PiP）、后台策略 |
| 对话解析、统一 Item/Block 模型与完整消息缓存 | 渲染：Compose / SwiftUI / WPF / GTK、Markdown、富文本与 diff 展示 |
| 领域状态：主机、连接、agent、聊天消息、状态行与通知游标 | 布局、导航、手势、动画与虚拟渲染窗口 |
| 本地持久化：主机、信任指纹、会话缓存、本机订阅状态 | 输入采集：触控、键盘、输入法、仿真鼠标与指针捕获 |
| 设备密钥生成与 SSH 认证 | 面向用户的文案与本地化资源 |
| 串流配对、会话、组帧与视频/音频输出 | 硬件解码与显示/播放：MediaCodec / VideoToolbox / Media Foundation / AudioTrack |
| 重连、超时、重试策略与会话图谱关系 | 敏感数据安全存储：Android Keystore / Apple Keychain / Windows DPAPI |
| 主机侧剪贴板经 SSH 读写 | 本机剪贴板、系统通知展示、权限申请与系统分享 |

易划错边界的明确约定：

- **状态行 `ActivityLine`**：Rust 决定展示哪一条及刷新节流（优先级、每秒最多一次、每条至少停留 1.5 秒），输出结构化数据；原生层按本地化模板拼句并决定样式。
- **设备私钥**：Rust 生成与使用；原生层通过 `SecretStore` 回调提供加密存储，私钥明文不得写入 `kite-store`。
- **接管输入**：原生采集事件并映射为统一的源桌面坐标/输入事件，Rust 负责编码与发送。

---

## 2. 三条跨边界通道

| 通道 | 方向 | 用途 | 机制 |
| --- | --- | --- | --- |
| 命令 | 原生 → Rust | 用户操作（连接、发消息、审批、接管、配对等） | UniFFI async 方法；Windows 通过进程内 C ABI 在原生工作线程调用 |
| 状态 | Rust → 原生 | 视图模型快照与聊天增量事件 | UniFFI observer 回调；Windows 回调通知 revision 后由原生拉取最新快照/增量 |
| 媒体 | 双向 | 视频帧与音频帧（Rust → 原生）、串流输入（原生 → 原生→Rust） | Android JNI + direct ByteBuffer；Apple 与 Windows C ABI |

### 2.1 UniFFI / C ABI 接口约定

- **单入口对象**：原生进程启动时创建唯一 `KiteCore`，注入数据目录、缓存目录、设备信息与 `SecretStore`；整个进程复用同一实例，不随界面重建。
- **类型与命令**：长生命周期对象用 Object，数据用 Record，状态用 Enum；命令方法以动词开头，耗时操作必须 async，同步方法不得做 I/O。
- **快照与增量**：各视图模型带单调递增 `revision`，原生丢弃旧版本；聊天正文采用增量更新。
- **回调线程纪律**：Rust 在工作线程触发回调，原生实现必须立即返回，不得阻塞也不得同步回凋 Rust；UI 更新切回主线程。
- **错误与 panic**：导出函数返回 `Result<T, KiteError>`；`KiteError` 为稳定错误码枚举，字段不得命名为 `message`（避免与 Kotlin `Throwable.message` 冲突），`details` 仅作次要诊断信息。Release 构建保持 `panic = "unwind"`，禁止用 panic 做正常控制流。
- **数据边界**：ID 用不透明字符串，时间用 Unix 毫秒（`i64`），小字节数组限 64 KB 内，禁止跨边界传递裸指针。
- **同次构建**：绑定每次随库生成且不提交，Rust 与原生同版发布，FFI 不保留旧入口兼容层；但上游 `herdr 0.9.5` 协议必须兼容。

---

## 3. 媒体通道、远程桌面交互与音频

### 3.1 媒体通道与解码指标

- `kite-stream` 只依赖抽象 `MediaSink` trait 与统一输入事件，不感知 JNI 或平台图形 API。
- Surface、解码器配置与渲染生命周期归原生层；帧回调内不得分配大对象或阻塞等待 UI 线程。
- 同一路串流的输入事件必须走同一有序通道。
- **流畅与响应优先级**：保持源桌面分辨率，外网带宽预算优先调低压缩码率；Android 优先硬件解码，临时输入缓冲压力不得立即丢弃参考帧，允许丢弃已解码的陈旧显示帧追上实时画面。性能指标（接收/提交/显示帧率、排队 P95）仅采有界数值，不得采集画面、按键内容或账号信息。

### 3.2 桌面交互、输入合并与剪贴板（RFC 0004 / 0005 / 0010 / 0011）

- **观看与操作分离**：观看模式缩放和平移不发送输入；操作模式不自动暂停 agent，agent 暂停与桌面接管完全独立。
- **源坐标系与紧凑控件**：显示与输入共用统一源桌面坐标系。紧凑控件按“桌面 / 可收起触控板（仅上下分屏）/ Chat / 底部输入框”排列；全屏保留鼠标形状控件与右侧操作栏，展开后鼠标箭头与左右键保持固定屏幕相对偏移，经 `DesktopViewport` 反算源桌面像素。
- **固定合并窗口**：操作模式的鼠标移动与滚轮使用固定 `16ms` 合并窗口（连续动作不得重置期限）；文字输入使用固定 `200ms` 窗口；点击和按键屏障按序立即提交。切回只看、失焦、退后台或销毁串流时立即取消待发并释放已按状态。
- **双向文本剪贴板**：限绑定主机身份、当前操作会话、大小与超时上限，不落盘记录剪贴板内容。
- **Surface 独立生命周期（Android）**：Surface 重建会重建对应 renderer，但工具栏控件引用与文字 flush 回调仅在该控件真正退出 Compose 时按控件身份清理。

### 3.3 远程桌面声音（RFC 0016）

- 声音经专用媒体通道传输，Rust 管传输与格式，Android 使用独立工作线程和有界缓冲解码 Opus 并经 `AudioTrack` 播放（不阻塞视频、不记录声音内容）。
- 静音、音量、音频焦点与播放生命周期归原生设置；声音开关独立于观看/操作模式，关闭串流释放音频资源。

---

## 4. 多端原生平台约定

### 4.1 共同规则

- ViewModel 只做三件事：映射只读快照为 UI 状态、转发用户操作为 Rust 命令、选择本地化文案；不得在 ViewModel 中写网络、协议或业务规则，不得就地修改 Rust 快照。
- 自动生成的 FFI 绑定禁止手改；所有可见文案放在平台字符串资源中。

### 4.2 Android

- 包结构：`io.github.wenshiqi0.kite.{core, ui, data, media}`。
- 文案在 `strings.xml`，敏感数据用 Android Keystore 加密存储。
- 使用 `MediaCodec` + `SurfaceView` + 预分配 direct `ByteBuffer`；后台在系统 PiP 中保留视频否则暂停；前台服务管理 SSH 生命周期，并妥善处理快速失败时先升前台再停止服务的顺序；提供默认网络状态事实，服务重建按 Rust 持久化意图恢复。

### 4.3 iOS（iPhone & iPad，RFC 0002）

- 使用 XcodeGen 维护 `ios/project.yml`（最低 iOS 17，单通用 App 覆盖 iPhone/iPad），编译前脚本 `ios/scripts/build-rust.sh` 构建静态库并输出绑定到 `ios/Generated/`。
- 目录：`ios/Kite/{App, UI, Data, Resources}`；文案用 `Localizable.xcstrings`；密钥存 Keychain（`ThisDeviceOnly`，不同步 iCloud）；UI 更新限 `@MainActor`。
- **布局原则**：必须由 `size class` 和可用宽度决定，不得按设备型号判断（iPad 窄分屏等同 iPhone 布局）；硬件键盘与指针按连接状态检测。

### 4.4 macOS（RFC 0015 / 0027-mac / 0028-keychain / 0029）

- macOS 14+ 原生 SwiftUI/AppKit 客户端，XcodeGen 维护 `macos/project.yml`，同次生成 FFI 绑定。
- Apple 媒体通过 FFI 共享 `media` 模块 C ABI 回调与不透明整数 handle 传输，由 VideoToolbox 解码、Metal 展示、原生音频播放。
- 客户端与本机主机双角色独立：主窗口连接远程机器，本机服务窗口通过结构化 `kite-node` CLI 管理 launchd 用户服务；关闭应用不停止后台节点或 agent。
- 本机开发版采用固定证书签名 + 只安装一次、按需启动的原生钥匙串组件（校验证书与 bundle ID，通过私有管道读写两个既定条目），正常读写无弹窗，禁止静默降级或重建设备身份。

### 4.5 Windows（RFC 0031）

- Windows x64 使用 C# / WPF 实现**纯客户端**（支持远程主机管理、独立多主机连接、聊天、审批、问题、附件和远程桌面），**不包含**本机主机角色（不打包 Node、Herdr、Sunshine 或 Bun）。
- 通过 `kite-ffi` 的进程内 C ABI（同次构建、显式生命周期、捕获 panic）与独立 C 媒体通道通信；原生工作线程执行命令，UI 线程仅呈现；设备秘密使用当前用户 DPAPI 保存。
- Linux MinGW 交叉出包与 Windows 实机验收须分开记录。

### 4.6 Linux 原生主机面板

- `kite-desktop`（Rust/GTK）仅作主机控制面板与 X11 托盘展示，通过结构化 CLI 调用 `kite-node`，不得引入 provider 逻辑或迁入 Bun/TypeScript。关闭面板或退出托盘不得停止后台服务。

---

## 5. 历史升级与兼容基线（M1 / Herdr）

- M1 的 `SharedPreferences` / `UserDefaults` 仅作只读迁移源，迁移幂等且不覆盖冲突；`SecretStore` 复用原有设备密钥材料，不重建身份。
- 上游 `herdr 0.9.5` 协议必须保持兼容；跨版本升级时 FFI 库与绑定同次生成，不维护废弃 FFI 入口。
- 串流与桌面验收必须分别核验真实 Herdr API、持续视频接收、实际解码呈现与输入到达，不能仅以编译通过或数据包计数代替端到端画面与操作证据。
