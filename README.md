# kite

kite 是 [herdr](https://github.com/herdrdev/herdr) 的原生移动客户端：主界面是每个 agent 的聊天面板，agent 做 computer use 时可以随时调出远程桌面观看或接管。

- 工程规范（crate 划分、Rust 与原生界面的分界）：[SPEC.md](SPEC.md)
- 设计提案：[docs/rfcs/](docs/rfcs/README.md)，整体架构见 [RFC 0001](docs/rfcs/0001-architecture.md)

当前版本：**Android 持久 herdr 连接与 Sunshine 远程桌面**。

Android 交付包使用 Alpha 版本，并在打包后发布至 GitHub prerelease。版本、固定签名、源码标签和发布命令见 [Alpha 发布方法](docs/releases.md)。

- Android 可保持 SSH 连接、查询 herdr agent、读取终端输出和发送提示；可在 App 内配对 Sunshine、观看 H.264 视频，并显式进入键鼠接管。
- M1 主机配置和全部 SSH 信任记录迁入 Rust；Android Keystore / iOS Keychain 中原有设备密钥继续复用。
- iOS 已同步新的持久 Core / SecretStore 契约和原有连接界面；iOS 视频呈现尚未接入。
- 结构化聊天记录、音频、画中画、浮窗和完整终端单元格渲染仍待实现。终端输出面板不冒充聊天记录。

具体范围和验收边界见 [RFC 0003](docs/rfcs/0003-persistent-desktop.md)。

## 仓库结构

```
SPEC.md               工程规范
Cargo.toml            Rust workspace 根
crates/               Rust 内核，所有平台共用
├── kite-ssh/         SSH 连接、ed25519 密钥、严格的主机密钥校验、执行命令
├── kite-herdr/       herdr 查找、桥接与 JSON API
├── kite-store/       主机、公开信任记录、配对公钥与 M1 原子迁移
├── kite-stream/      Sunshine 配对/控制、Moonlight C 传输与平台无关媒体接口
├── kite-app/         持久 Core、状态快照、唯一 Tokio 运行时与任务生命周期
├── kite-ffi/         唯一对外的库：通过 UniFFI 导出给 Kotlin / Swift
└── uniffi-bindgen/   生成绑定代码的命令行工具
android/              Android 应用（Kotlin + Jetpack Compose）
ios/                  iOS 应用（SwiftUI，iPhone 和 iPad 通用，XcodeGen 工程）
docs/rfcs/            设计提案（RFC）
```

## Rust 内核

需要 Rust stable、Perl 和 make（用于 OpenSSL 源码构建）。首次克隆后初始化固定版本的原生依赖：

```bash
git submodule update --init --recursive
```

Moonlight 必须连同其指定的 ENet / nanors 子模块一起构建，不链接系统 ENet。基础层和领域层测试使用本地夹具，不访问真实主机：

```bash
cargo test --workspace
```

## Android

环境要求：

- Android 编译目标：`rustup target add aarch64-linux-android`
- [cargo-ndk](https://github.com/bbqsrc/cargo-ndk)：`cargo install cargo-ndk`
- JDK 17 或更新版本
- Android SDK（compileSdk 37）和 NDK（版本见 `android/app/build.gradle.kts` 中的 `ndkVersion`），并设置 `ANDROID_HOME`

编译调试版 APK。Gradle 会先用 cargo-ndk 编译 Rust 内核，再生成 Kotlin 绑定：

```bash
cd android && ./gradlew :app:assembleDebug
```

安装到已连接的手机或模拟器：

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

默认只为 `arm64-v8a` 编译和打包；APK 和 Rust 内核共用 `kite.rustAbis` 配置。要在 x86_64 模拟器上运行，先执行 `rustup target add x86_64-linux-android`，再在 `android/gradle.properties` 里把 `kite.rustAbis` 改成 `arm64-v8a,x86_64`。

## iOS

环境要求：

- Xcode 16 或更新版本，以及 [XcodeGen](https://github.com/yonaskolb/XcodeGen)：`brew install xcodegen`
- iOS 编译目标：`rustup target add aarch64-apple-ios aarch64-apple-ios-sim`

第一次先编译 Rust 内核、生成 Swift 绑定，再生成 Xcode 工程：

```bash
ios/scripts/build-rust.sh
```

```bash
cd ios && xcodegen generate && open Kite.xcodeproj
```

之后每次在 Xcode 里构建，都会先自动执行 `build-rust.sh`，只编译当前目标平台（模拟器或真机）。Debug 配置用 Rust 的 dev profile，Release 配置用 release profile。

不装模拟器运行时、只验证能否编译时，按 target 构建可以绕开 Xcode 对 iOS 平台组件的检查：

```bash
xcodebuild -project ios/Kite.xcodeproj -target Kite -sdk iphonesimulator ARCHS=arm64 ONLY_ACTIVE_ARCH=YES CODE_SIGNING_ALLOWED=NO build
```

iPhone 和 iPad 的布局按窗口宽度切换，不按设备型号切换：iPad 在分屏或侧拉的窄窗口里用的是 iPhone 的单列布局。规则见 [RFC 0002](docs/rfcs/0002-ios.md)。

## 连接 herdr

1. 打开 App，它会在本机生成一把 ed25519 密钥。私钥不会离开设备：Android 上用 Keystore 加密保存，iOS 上存进只在本机可用的 Keychain。
2. 点「复制公钥」，把公钥加到主机上对应用户的 `~/.ssh/authorized_keys`。
3. 填写主机地址、端口和用户名，点「连接 herdr」。
4. 第一次连接时，App 会显示主机密钥指纹。在主机上运行 `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` 核对一致后，点「信任并连接」。
5. 看到「已连接 herdr」及实际版本，SSH 会话会保持；主机上需要有运行中的 herdr 服务。agent 列表来自 herdr JSON API。

App 会在 `command -v herdr`、登录 shell 的 PATH 和常见安装位置（`~/.local/bin`、Homebrew、Nix 等）里查找 herdr。装在别处时，在「高级设置」的「herdr 路径」里填写完整路径。

在 Android 模拟器里，`10.0.2.2` 指向运行模拟器的这台电脑。iOS 模拟器和 Mac 共用网络，直接用 `127.0.0.1` 即可。

## Android 远程桌面

1. 主机安装并启动 Sunshine，保持可捕获的桌面会话。端口仅在局域网 / Tailscale 等私有网络可达，关闭 UPnP，不对公网开放。
2. 连接 herdr 后点「打开远程桌面」。Sunshine 地址默认与 SSH 主机相同，可在高级设置中修改；视频直接连接此地址，不通过 SSH 的 TCP 通道。
3. 首次连接显示 PIN 和本次客户端名称。在 Sunshine 管理页面选择对应请求并输入 PIN。主机具有 `~/.config/sunshine/admin-credentials.json` 时，可主动点「自动确认配对」；凭据只在已验证的 SSH 通道中使用，不进入普通存储或日志。
4. 配对证书持久化，后续重开桌面无需再次配对。先进入仅观看模式；点「接管」才发送输入。支持点按、长按拖动、双指右键 / 滚动、外接键盘，以及 UTF-8 文字输入；工具栏也提供右键、滚轮和常用按键。
5. 点「退出接管」释放按下的键鼠。返回主机页停止视频但保留 SSH；App 进入后台暂停视频，前台服务保持 SSH，回到桌面页恢复视频。连接存活遵循 Android 前台服务限制，可从通知或 App 断开。

控制与配对请求通过 SSH `direct-tcpip` 通道，SSH 服务和公钥选项必须允许 TCP 转发。若公钥使用 `restrict`，须另外启用 `port-forwarding`；普通无约束的公钥条目不需修改。未知 SSH 指纹必须确认，变化的指纹必须拒绝。

本轮以 herdr **0.9.5**、Sunshine **v2026.914.233613** 验证。自动确认使用当前版本的 `/api/pin` 请求 ID，不发送旧版无 ID 的兼容请求。客户端默认保持主机桌面源尺寸，手机端等比适配，旋转手机不改变源视频尺寸。当前尺寸自动探测支持同一用户运行的 Linux X11 Sunshine，需要 `ss` 与 `xrandr`；其他捕获后端尚未接入，探测失败会明确报错。

优先 H.264、60fps；为维持交互帧率，解码能力不足时允许最多 15% 边长的等比降采样，再不足才尝试 30fps。码率随视频像素数和帧率调整，1080p60 约 15.5Mbps，上限 40Mbps；启用 MediaCodec 低延迟。当前不进行网络拥塞触发的动态分辨率切换，不解码或播放音频；同时仅允许一路桌面。请求帧率不等于主机实际刷新率或设备实测帧率，其他主机、版本、输入法和物理设备须分别验证。

在 Sunshine 运行期间调整 X11 桌面模式后，需要检查视频和鼠标坐标是否一致。本次验证发现该版本的 XTEST 输入映射保留了启动时的桌面尺寸；切换到 1080p 后重启 Sunshine 才刷新映射。仅重开视频未修复这个主机状态，重启 Sunshine 不需要重启 Xorg 或桌面应用。

## 手动端到端测试夹具

主要验证工具位于 `crates/kite-app/examples/`：

- `desktop_probe.rs` 明确连真实主机，验证 herdr API、配对和编码视频接收；它不是离线单测，也不声称视频已解码。
- `desktop_fixture.py` 是可选的 Linux GTK3 动态窗口，记录鼠标/键盘计数及指定中文测试词是否匹配，不记录任意输入文字；退出恢复原焦点，不改变显示模式。它不是产品运行时依赖。
- `android/ui-probe` 是独立的 `testOnly` 测试包，可通过 Accessibility 将中文写入手机端唯一的文字输入框，不重启产品进程；不包含在生产 APK。

常规检查：

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd android && ./gradlew :app:assembleDebug :app:lintDebug :app:testDebugUnitTest
```

Android 目前没有 JVM 单测源，`testDebugUnitTest` 的 `NO-SOURCE` 不算单测通过。桌面验收必须验证真实配对、MediaCodec 呈现、观看模式输入隔离、主机输入回执及生命周期；编译成功、服务在线或仅收到编码包不能代替这些证据。

## 许可证

[GPL-3.0](LICENSE)。原生核心依赖的固定版本与许可证见 [NOTICE.txt](NOTICE.txt)。
