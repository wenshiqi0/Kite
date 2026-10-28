# RFC 0001：kite 架构设计

> 编号：0001 · 状态：已采纳 · 创建：2026-10-28
> 范围：技术栈、系统架构、herdr 与 Sunshine 集成、交互设计（含桌面的几种呈现方式和状态行规则）
> 由本 RFC 产生的硬性规则已汇总进 [SPEC.md](../../SPEC.md)。

## 1. 目标

kite 是 [herdr](https://github.com/herdrdev/herdr) 的原生移动客户端。在手机上用 Termius 这类 SSH 客户端跑 herdr，显示效果和交互都很差；kite 把 herdr 的能力重新做成原生界面。

- **先做 Android**，之后是 iOS 和更多系统。
- **主交互是每个 agent 的聊天面板。**
- **远程桌面**：agent 在主机的可视桌面上做 computer use 时，可以随时观看，也可以全屏手动操作。画面由主机上的 [Sunshine](https://github.com/LizardByte/Sunshine) 采集。

现有的第三方 herdr 客户端（Whip、Herdr Mobile、Sherpr、Heeler）能处理终端和对话，但都看不到主机桌面。这是 kite 的主要差异点。

**不做的事：**

- 不在手机上运行 agent，也不替代 herdr。
- 不对公网暴露任何端口，不提供 kite 自己的中转服务（iOS 推送方案另议，见第 10 节）。

## 2. 技术栈总览

| 层 | 选型 | 说明 |
|---|---|---|
| 共享内核 | Rust | SSH、herdr 协议、对话记录解析、Moonlight 协议、应用状态，所有平台共用 |
| SSH | russh | 纯 Rust，基于 tokio，移动端不依赖 OpenSSL |
| herdr 协议 | 复用 herdr 源码中的协议类型 | herdr 是 Apache-2.0，复制类型定义可保证 bincode 编码逐字节一致 |
| Moonlight 协议 | moonlight-common-c，Rust 通过 FFI 调用 | 已定，见 5.1 |
| 许可证 | GPL-3.0 | kite 不做商业付费；与 moonlight-common-c（GPL-3.0）和 herdr（Apache-2.0）都兼容 |
| 绑定 | UniFFI（状态和指令）+ 手写 JNI（视频帧和高频输入） | UniFFI 同时生成 Kotlin 和 Swift |
| Android 界面 | Kotlin + Jetpack Compose | 终端用自定义 View；视频用 SurfaceView / TextureView + MediaCodec |
| iOS 界面（后续） | Swift + SwiftUI | 终端用 CoreText / Metal；视频用 VideoToolbox |
| 主机侧 | herdr（已有）+ Sunshine | 除此之外不要求主机安装任何东西 |

原则：**SSH 和所有逻辑都在 Rust 内核里，渲染和交互用各平台的原生语言。** herdr 服务端已经用 ghostty-vt 完成了终端模拟，客户端只需把单元格画出来，所以各平台原生渲染的成本不高。

## 3. 系统架构

```
┌──────── Android（Kotlin）─────────────┐    ┌──── iOS（Swift，后续）────┐
│ Compose：主机 / agent 列表 / 聊天      │    │ SwiftUI                   │
│ 终端：自定义 View 画单元格（兜底）      │    │ 终端：CoreText / Metal     │
│ 桌面：SurfaceView / TextureView        │    │ 桌面：VideoToolbox         │
│       + MediaCodec 硬解                │    │       + 画中画             │
│ 前台服务：保持连接、通知                │    │                           │
└──────┬───────────────────┬────────────┘    └──────────┬────────────────┘
  UniFFI（状态、指令）   JNI 直通（视频帧、高频输入）       UniFFI
┌──────┴───────────────────┴──────────────────────────────┴──────────┐
│ kite 内核（Rust，crates/）                                          │
│  kite-ffi     唯一对外的库：UniFFI 导出、JNI / C ABI 媒体通道           │
│  kite-app     多主机运行时、视图模型、命令                             │
│  kite-herdr   JSON API 客户端 + 客户端协议，维护每个 pane 的单元格       │
│  kite-agent   定位、增量读取、解析对话记录；推导状态行                  │
│  kite-stream  Sunshine 配对、Moonlight 会话、视频帧和输入               │
│  kite-ssh     连接、主机密钥校验、跳板机、多路通道、端口转发            │
│  kite-store   本地持久化：主机、已信任的主机密钥、缓存                  │
└──────┬─────────────────────────────────────────────┬──────────────┘
       │ SSH（TCP，一条连接多路复用）                   │ 私有网络（UDP）
       ▼                                             ▼
  主机                                           主机
  · herdr remote-api-bridge（JSON API）           · Sunshine 视频、音频、控制流
  · herdr remote-client-bridge（客户端协议）
  · 对话记录文件（增量读取）
  · Sunshine 配对和 Web API（端口转发）
  · 剪贴板命令
```

### 3.1 网络：两条路

| 流量 | 走哪条路 | 原因 |
|---|---|---|
| herdr JSON API、客户端协议 | SSH 执行桥接命令 | herdr 自带 SSH 桥，主机不用开端口 |
| 对话记录 | SSH 执行命令或 SFTP | 文件在主机本地 |
| Sunshine 配对、Web API | SSH 端口转发到主机本机端口 | 都是 TCP |
| 剪贴板同步 | SSH 执行命令 | Moonlight 协议没有剪贴板同步 |
| Sunshine 视频、音频、控制流 | 私有网络直连：Tailscale、WireGuard、ZeroTier 或局域网 | 都是 UDP（47998–48000 等端口，以所装版本的文档为准），SSH 转发不了；塞进 TCP 隧道一丢包就卡 |

- Sunshine 的端口**绝不对公网开放**。它在 2026 年 5 月发过一次关键安全更新。
- 国内网络下，Tailscale 打洞失败会走海外中继，延迟很高。可以自建 DERP 中继，或换 EasyTier / ZeroTier。

### 3.2 Rust 内核模块

crate 的分层、职责和依赖规则以 [SPEC.md](../../SPEC.md) 第 3 节为准，这里只说明各自做什么：

- **kite-ssh**：基于 russh。生成 ed25519 密钥；严格校验主机密钥；支持跳板机；保活和断线重连。用到两种通道：执行命令（herdr 桥接、读文件、剪贴板）和端口转发（Sunshine 的 TCP 端口）。
- **kite-store**：本地持久化主机配置、已信任的主机密钥和对话缓存。私钥明文不进这里，由原生的安全存储保管。
- **kite-herdr**：管理两条桥接。JSON API 负责 agent 列表、事件订阅、发消息、读屏、发按键；客户端协议负责帧编解码、结构快照、每个 pane 的单元格（接收全量和增量更新），以及语义化输入。
- **kite-agent**：通过 herdr 定位对话记录文件，经 SSH 增量读取，按 agent 类型解析成统一的消息格式；推导状态行（见 6.4）。
- **kite-stream**：Sunshine 配对（客户端证书）、启动和恢复会话、Moonlight 协议、把 H.264 / HEVC 帧交给平台解码器、发送输入、暂停和恢复串流。
- **kite-app**：编排上面这些 crate。聚合多台主机的应用状态，带版本号推送给界面；持有整个进程唯一的 tokio 运行时。
- **kite-ffi**：唯一编译成动态库的 crate，只做类型映射：UniFFI 导出、错误映射、媒体通道和平台日志，不含业务逻辑。

### 3.3 Rust 与原生代码的边界

谁负责什么、三条通道（命令、状态、媒体）的约定，见 [SPEC.md](../../SPEC.md) 第 4 节。要点：

- **状态和指令走 UniFFI**：连接状态、agent 列表、聊天消息、状态行、终端单元格增量。这些数据量小、频率低。
- **视频帧和高频输入走 JNI 直通**：UniFFI 的 Kotlin 绑定底层用 JNA，每次调用都有额外开销和拷贝。视频帧通过 direct ByteBuffer 交给 Kotlin 的 MediaCodec；指针移动这类每秒上百次的输入也走这条通道。moonlight-android 也是这种结构：C 库经 JNI 交给 Java 的 MediaCodec。
- 内核不持有任何界面对象，Surface 由原生层管理。
- 给用户看的文字都由原生层写，Rust 只交出结构化数据。

### 3.4 Android 层

| 部分 | 实现 |
|---|---|
| 工程 | 单 Activity + Compose 导航；Rust 用 cargo-ndk 编译 arm64-v8a（模拟器再加 x86_64）；uniffi-bindgen 生成 Kotlin 绑定 |
| 最低版本 | 建议 minSdk 26（支持画中画）；自动进入画中画需要 Android 12+ |
| 聊天 | Compose 列表：Markdown、diff、截图，工具调用默认折叠 |
| 终端（兜底） | 自定义 View 嵌进 Compose；相同样式的格子合并成一段绘制，字形做缓存 |
| 桌面 | 见 5.4 |
| 后台 | 前台服务保持 SSH 连接；agent 变成 blocked 或 done 时发通知；常驻通知显示状态行。Android 14+ 必须声明前台服务类型，具体类型待定 |
| 密钥 | 内核生成 ed25519；私钥用 Android Keystore 里的密钥加密后保存 |
| 屏幕方向 | Activity 自己处理方向变化，旋转时不重建界面，视频不中断 |

### 3.5 iOS（后续）

- 同一个 Rust 内核打包成 xcframework，UniFFI 生成 Swift 绑定。
- 界面用 SwiftUI；终端用 CoreText 或 Metal 绘制；视频用 VideoToolbox 解码、AVSampleBufferDisplayLayer 显示。
- 画中画：从 iOS 15 起，`AVPictureInPictureController` 支持把 AVSampleBufferDisplayLayer 作为内容源，四种呈现方式可以照搬。
- 外接鼠标用 GCMouse。
- iOS 切到后台后保不住 SSH 连接，后台通知需要另做推送（见第 10 节）。

## 4. herdr 集成

以下内容基于 herdr v0.9.5 的源码（commit `2713feb`，2026-10-27）。

### 4.1 SSH 桥接

| 命令 | 作用 |
|---|---|
| `herdr remote-api-bridge` | 把 stdio 接到主机上的 herdr API socket（JSON API）。`--check` 会输出 `herdr-api-bridge-v1`，可用来探测主机是否装了 herdr |
| `herdr remote-client-bridge` | 把 stdio 接到 herdr 的客户端 socket（二进制客户端协议）。服务端没运行时会自动启动它；要求服务端的 endpoint protocol generation 为 1 |

### 4.2 JSON API

接口定义见 herdr 仓库的 `docs/next/api/herdr-api.schema.json`（protocol 22）。kite 用到的方法：

| 用途 | 方法 |
|---|---|
| agent 列表和状态 | `agent.list`、`agent.get`、`events.subscribe`（如 `pane.agent_status_changed` 事件） |
| 发消息 | `agent.prompt` |
| 卡住时读屏和回答 | `agent.read`（`source: visible`）、`agent.send_keys` |
| 启动 agent | `agent.start` |
| 工作区结构 | `workspace.list`、`tab.list`、`pane.list`、`session.snapshot` |

agent 信息里 kite 关心的字段：

- `agent_status`：idle、working、blocked、done、unknown。
- `agent_session`：`kind` 为 `path` 或 `id`，`value` 是会话文件路径或会话 ID。
- `terminal_title`、`cwd`、`completion_seq`。

`agent.explain` 是用来排查 herdr 状态检测的（解释它为什么判定 agent 在工作或卡住），不适合给用户看。

JSON API 在 socket 上如何分隔消息，实现时以源码为准。

### 4.3 客户端协议（用于终端兜底视图）

- 帧格式：`[u32 小端长度][bincode 负载]`，bincode 用 standard 配置。单帧上限 2 MB，带 Kitty 图形时 32 MB。
- `ClientMessage` 和 `ServerMessage` 的变体顺序在 endpoint generation 1 中冻结。新能力通过 `EndpointControl { kind, data }` 扩展（`data` 是 JSON），不认识的 `kind` 一律忽略。
- 编解码器：`shell.snapshot.v1`、`shell.surface.v1`、`shell.input.semantic.v1`、`shell.blob.v1`。可协商的能力包括 `surface_reuse`、`surface_delta`、`surface_scroll`、`health_check` 等。
- 服务端下发：`ClientShellSnapshot`（workspace、tab、pane、agent 的结构化快照），以及 `PaneSurface` 和 `PaneSurfacePatch`（已经渲染好的单元格和增量）。
- 客户端发送：`ClientShellPaneInput { pane_id, events }`，事件包括按键、`TextCommit`、鼠标和 `Paste`。
- 实现方式：从 herdr 源码（`src/protocol/wire.rs`、`src/protocol/endpoint.rs` 等）复制协议类型，保证编码一致。握手细节以 `src/client/handshake.rs` 为准。

### 4.4 聊天面板的数据来源

herdr 本身没有聊天面板，kite 按下面的步骤组装：

1. 通过 `agent.list` 和 `events.subscribe` 拿到每个 agent、它的状态和 `agent_session`。
2. `kind` 为 `path` 时直接得到对话记录文件路径（例如 Claude Code 通过 hook 上报的 transcript 路径）；为 `id` 时，按各 agent 的约定目录查找。
3. 经 SSH 按字节偏移增量读取 JSONL 文件。
4. 按 agent 类型解析成统一的消息格式：用户消息、回复、工具调用及结果、截图、diff。先支持 Claude Code 和 Codex。文件格式会随 agent 版本变化，解析要容忍不认识的字段。
5. 发消息用 `agent.prompt`。权限确认的提示显示在终端界面上，不一定写进对话记录，所以用 `agent.read` 读当前屏幕、解析出选项，再用 `agent.send_keys` 回答。

## 5. 远程桌面（Sunshine）

### 5.1 Moonlight 协议的实现方式（已定）

kite 以 GPL-3.0 发布，所以可以直接使用现成的 GPL 实现。

| 方案 | 成熟度 | 许可证 | 结论 |
|---|---|---|---|
| [moonlight-common-c](https://github.com/moonlight-stream/moonlight-common-c)，Rust 通过 FFI 调用 | Moonlight 官方各平台客户端都在用；支持 AV1、视频加密、参考帧失效 | GPL-3.0 | **采用** |
| [moonlight-common-rust](https://github.com/MrCreativ3001/moonlight-common-rust)：纯 Rust，网络层可替换，也带 C 库绑定 | 很新；已支持 Sunshine、Apollo 和 H.264 / H.265，还没有 AV1 和视频加密 | GPL-3.0 | 以后成熟了可替换 |
| 自己实现，参考 [moonshine](https://github.com/hgaiser/moonshine) | 工作量大 | BSD-2-Clause | 不需要了 |

- moonlight-common-c 要求使用它自带的那个 ENet 版本（git 子模块），不能链接系统里的 libenet。
- kite-stream 的对外接口保持抽象，不暴露 moonlight-common-c 的类型，方便以后换成纯 Rust 实现。

### 5.2 配对与管理

- 配对和 Sunshine Web API 都是 TCP，经 SSH 端口转发访问主机本机的端口。
- 目标是一键配对：通过 Web API 自动提交 PIN（待验证）。
- 视频流走私有网络直连。kite 要知道主机在私有网络上的地址，并检测是否连得通。

### 5.3 串流参数

- 按观看需求请求参数，例如 1080p、30 帧、5–10 Mbps；网络差时降码率。
- 看 computer use 不需要声音，先不做音频。
- 浮窗收起一段时间后暂停串流，重新展开时恢复。

### 5.4 解码与渲染（Android）

- 只建一个 MediaCodec 解码器，硬解 H.264 / HEVC。
- 分屏和全屏这类固定位置的布局用 SurfaceView，延迟最低；浮窗要圆角和拖动，用 TextureView。两者之间用 `MediaCodec.setOutputSurface()`（API 23+）切换，不用重连，也不用等关键帧。
- 视频视图用 Compose 的 `movableContentOf` 在不同布局之间挪动，视图不会被销毁。
- 解码器的低延迟配置因芯片而异。moonlight-android 的解码器实现（GPL-3.0）可以直接复用，保留原版权声明。

### 5.5 输入（接管之后）

| 能力 | 做法 |
|---|---|
| 指针 | 两种模式可切换。触控板模式：单指点是左键，双指点是右键，双指滑动是滚轮，长按是拖拽。直接点按模式：点哪里就点到哪里（绝对坐标） |
| 键盘 | 系统输入法直接打中文（Moonlight 协议能发 UTF-8 文字，各系统上 Sunshine 的支持程度待实测）；快捷键栏：Esc、Tab、Ctrl、Alt、Cmd、方向键、F 键和组合键 |
| 剪贴板 | 走 SSH：macOS 用 `pbcopy` / `pbpaste`，Linux 用 `wl-copy` / `xclip` |
| 外接设备 | 蓝牙键鼠，以及三星 DeX 这类桌面模式：用 Android 的指针捕获拿到鼠标相对移动 |
| 本地视野 | 双指缩放、平移 |

### 5.6 主机侧要求

- Sunshine 支持 macOS、Linux 和 Windows。
- macOS 上要给 Sunshine 开"屏幕录制"权限；要注入键鼠，还要开"辅助功能"权限。
- 没接显示器的主机需要 HDMI 欺骗器或虚拟显示器，否则抓到的画面分辨率很低，甚至黑屏。
- Sunshine 的端口只对私有网络开放。

## 6. 交互设计

### 6.1 主界面：聊天面板

- **顶部**：当前 agent 的名称和状态，可切换 agent。所有主机上的全部 agent 放在侧边栏，按状态排序，blocked 排最前。
- **中间**：消息列表，包括回复（Markdown）、工具调用（默认折叠）、diff 和截图。
- **底部**：原生输入框，支持中文输入法、语音、粘贴和附件。agent 卡住时，输入框上方出现确认按钮。
- **兜底**：一键切到原始终端视图（herdr 客户端协议）。

### 6.2 桌面的四种呈现方式

| 方式 | 布局 | 适合的场景 |
|---|---|---|
| 浮窗 | 叠在聊天面板上，可拖动、贴边、缩放，也可收成屏幕边缘的小标签 | 以聊天为主，偶尔瞄一眼桌面 |
| 上下分屏 | 竖屏，上面桌面、下面聊天，中间的把手可上下拖动调比例，类似直播间 | 边看 agent 操作边聊 |
| 全屏 | 横屏，桌面占满屏幕 | 看清细节，或手动操作 |
| 画中画 | 离开 App 后的系统小窗 | 切到别的 App 时继续看 |

切换方式：

- **分屏 ↔ 全屏**：把手机横过来或竖回去，或者点按钮。提供方向锁定开关。
- **分屏 ↔ 浮窗**：把顶部的桌面区往下拖，它就变成浮窗；点浮窗上的"停靠"回到顶部。
- **任何方式下离开 App**：自动转成画中画（Android 12+ 自动进入）。
- 全程只有一路视频流，切换不重连、不卡顿。

分屏时，顶部桌面区支持双指缩放、平移和双击放大。计划中的"跟随 agent"功能：点聊天里某次 computer use 工具调用，桌面区自动放大到那次点击的位置（依赖坐标换算，待验证）。

平板和大屏横屏下，聊天和桌面改成左右并排，不用浮窗。

### 6.3 三个互不影响的维度

| 维度 | 取值 | 怎么改变 |
|---|---|---|
| 布局 | 浮窗 / 分屏 / 全屏 / 画中画 | 用户手势、旋转手机、离开 App |
| 输入 | 只看（默认）/ 接管 | 只能由用户点"接管"或"结束接管"切换 |
| agent 状态 | 以 herdr 报告的实际状态为准 | 由 agent 自己决定；只有用户点"暂停 agent"才会打断它 |

- 进入全屏既不会接管，也不会暂停 agent。
- 只看时，手势只用来在本地缩放和平移，不会发到远程桌面。
- 接管也不会暂停 agent。

### 6.4 状态行：agent 现在在干嘛

全屏时显示在左上角，用一句话说明 agent 正在做什么。

生成规则：按优先级从上往下，取第一条能用的。

| 优先级 | 来源 | 示例 |
|---|---|---|
| 1 | herdr 报告 agent 卡住（blocked） | "等你确认：允许执行 psql 删除命令？"，黄色显示。点一下就地弹出允许 / 拒绝，不用退出全屏 |
| 2 | 对话记录里最新一次工具调用，套模板转成一句话 | computer 类调用写成"点击「登录」""输入文字""滚动页面"；Bash 写成"运行 npm test"；编辑文件写成"修改 src/login.ts" |
| 3 | agent 最新一条回复的第一句 | "页面已打开，正在填写测试账号" |
| 4 | herdr 拿到的终端标题，加上状态 | Claude Code 会把当前任务的主题写进终端标题 |

agent 做完时显示"已完成：……"，用绿色。

显示规则：

- 最多一秒刷新一次，每句话至少停留 1.5 秒，换句时淡入淡出，避免闪烁。
- 半透明底，叠在任何桌面画面上都看得清。如果挡住了内容，可以拖到别的角。
- 点一下展开最近 5 步；从右边缘可以拉出完整的聊天抽屉。

一处推导，多处复用：kite-agent 按上面的优先级和显示规则决定"现在显示哪一条"，输出结构化的状态行（例如"blocked + 提示内容""工具调用 + 动作 + 目标"）。原生层按模板把它写成一句话，同一条状态行同时用在分屏模式的标题、浮窗底部、画中画和 Android 常驻通知里。文案放在原生层，是为了走各平台自己的本地化机制（见 SPEC.md 第 4.1 节）。

先用模板生成：即时，也不花钱。以后如果嫌句子不够自然，再加一个可选的小模型润色。

### 6.5 接管：人的输入和 agent 的输入

**人和 agent 的输入是两条独立的通道。** 人的输入经 Sunshine 注入主机；agent 的输入由它自己的 computer use 工具产生。kite 不替 agent 发输入，也不拦截 agent 的输入。

- "接管"按钮在全屏的右上角。接管后底部出现快捷键栏，你的输入发到远程桌面。
- 接管期间 agent 照常运行，状态行继续更新。默认不提示冲突。

两者在主机上会不会互相干扰，取决于 agent 的 computer use 是怎么实现的：

| 实现方式 | 例子 | 和人的鼠标键盘 | kite 的处理 |
|---|---|---|---|
| 虚拟光标、后台操作 | Codex：每个 agent 线程有自己的光标，通过辅助功能树操作 App，不抢前台 | 互相独立，人可以同时操作 | 不提示冲突；只有两边操作同一个 App 时才提示 |
| 直接操作真实桌面 | Claude Code（macOS）：用辅助功能权限点击和输入，工作时会隐藏其他 App；在任何地方按 Esc 都会让它停下 | 共用系统光标和键盘 | 接管时提示"agent 正在操作真实桌面"，并提醒：快捷键栏里的 Esc 会让它停下 |

- kite 根据 herdr 报告的 agent 类型，以及对话记录里正在调用的工具，判断属于哪一种。
- "暂停 agent"按钮仍然保留，但只在用户主动点时才生效：通过 herdr 的 `agent.send_keys` 往 agent 的终端发 Esc，打断它当前这一轮。
- 结束接管时，可以选择往聊天里发一句话（例如"我手动处理完了，继续"），发送前可以修改。

### 6.6 自动行为

- 聊天里出现 computer use 工具调用时，自动弹出浮窗。如果已经在分屏或全屏，就保持不变。
- agent 卡住时，浮窗自动收到屏幕边缘，不挡住确认按钮。
- agent 一段时间不再用 computer use，浮窗自动收起；收起一段时间后暂停串流。

## 7. 安全

- **SSH**：严格校验主机密钥。遇到未知密钥，要用户确认指纹后才保存；密钥变了直接拒绝，直到用户手动删掉旧记录。
- **私钥**：不离开设备，用 Android Keystore 保护。
- **herdr**：不暴露任何端口，全部经 SSH 访问。
- **Sunshine**：端口只在私有网络内可达；Web UI 的账号密码保存在本地加密存储里。
- **不收集遥测**，没有 kite 自己的中转服务（iOS 推送方案确定之前）。

## 8. 仓库结构

```
kite/
├── SPEC.md               工程规范
├── Cargo.toml            Rust workspace 根
├── crates/
│   ├── kite-ssh/
│   ├── kite-store/       M2
│   ├── kite-herdr/
│   ├── kite-agent/       M2
│   ├── kite-stream/      M3
│   ├── kite-app/
│   ├── kite-ffi/         导出 UniFFI 接口和 JNI 通道
│   └── uniffi-bindgen/
├── android/              Gradle 工程（Kotlin + Compose）
├── ios/                  后续
└── docs/rfcs/            设计提案（RFC）
```

布局规则见 [SPEC.md](../../SPEC.md) 第 2 节。

## 9. 里程碑

| 里程碑 | 内容 | 完成标准 |
|---|---|---|
| M1 骨架 | Rust workspace、UniFFI、Android 空壳；加入 GPL-3.0 的 LICENSE；russh 连接、生成密钥、校验主机密钥 | 手机能连上主机，执行 `herdr remote-api-bridge --check` 成功 |
| M2 聊天面板 | agent 列表和状态订阅；读取并解析对话记录（Claude Code、Codex）；发消息；卡住时的确认按钮；状态行；前台服务和通知 | 不打开终端，也能和 agent 来回对话、处理确认 |
| M3 分屏观看 | Sunshine 配对；私有网络串流；MediaCodec 解码；上下分屏；只看 | 分屏里能实时看到主机桌面 |
| M4 全屏操控 | 横屏全屏；接管和输入；快捷键栏；剪贴板；冲突提示 | 能完整地手动操作远程桌面 |
| M5 浮窗和画中画 | 浮窗、边缘收起、画中画、自动弹出和收起 | 四种呈现方式能自由切换，且不重连 |
| M6 终端兜底 | herdr 客户端协议、终端视图 | 聊天面板处理不了的内容能在终端里处理 |
| M7 iOS | Swift 界面接入同一个内核 | 待定 |

## 10. 待定事项

1. **跑 computer use 的主机系统**（macOS / Linux / Windows）：影响 Sunshine 的权限设置、输入和剪贴板命令。
2. **私有网络方案**：Tailscale、自建 DERP、EasyTier 还是 ZeroTier。
3. **Sunshine 配对**：能否完全通过 SSH 自动完成（用 Web API 提交 PIN）。
4. **坐标换算**：computer use 工具调用里的坐标能否换算成实际屏幕坐标，"跟随 agent"依赖这一点。
5. **Sunshine 输入支持**：各系统上对 UTF-8 文字输入和触控事件的支持程度。
6. **Codex 的 computer use 能否在 herdr 里用**：公开资料显示它以插件形式提供在 Codex 桌面 App 里，`codex exec` 触发不了；herdr 里跑的 Codex CLI 能否使用、对话记录里怎么体现，要实测。
7. **agent 的虚拟光标会不会被 Sunshine 采集到**：会的话，观看时能同时看到 agent 和人的光标。
8. **herdr 协议细节**：JSON API 的消息分隔方式、客户端协议的握手，实现时以源码为准。
9. **Android 前台服务**：Android 14+ 用哪种前台服务类型，以及上架 Google Play 时的用途说明。
10. **iOS 后台通知**：用主机侧 hook 推送，还是自建中转。

## 11. 参考

- herdr：<https://github.com/herdrdev/herdr>
- Sunshine：<https://github.com/LizardByte/Sunshine>，文档 <https://docs.lizardbyte.dev/projects/sunshine>
- moonlight-common-c：<https://github.com/moonlight-stream/moonlight-common-c>
- moonlight-common-rust：<https://github.com/MrCreativ3001/moonlight-common-rust>
- moonlight-android：<https://github.com/moonlight-stream/moonlight-android>
- moonshine：<https://github.com/hgaiser/moonshine>
- Whip（参考同类客户端）：<https://github.com/kosumic/whip>
- Claude Code computer use：<https://code.claude.com/docs/en/computer-use>
- Codex App 的 computer use（第三方介绍）：<https://codex.danielvaughan.com/2026/04/17/codex-app-computer-use-macos-background-gui-automation/>
- Sunshine 2026 年 5 月安全更新：<https://www.gamingonlinux.com/2026/05/sunshine-game-streaming-tool-adds-vulkan-encoding-plus-xdg-pipewire-and-kwin-direct-screencast-capture/>
