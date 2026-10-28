# kite

kite 是 [herdr](https://github.com/herdrdev/herdr) 的原生移动客户端：主界面是每个 agent 的聊天面板，agent 做 computer use 时可以随时调出远程桌面观看或接管。

- 工程规范（crate 划分、Rust 与原生界面的分界）：[SPEC.md](SPEC.md)
- 设计提案：[docs/rfcs/](docs/rfcs/README.md)，整体架构见 [RFC 0001](docs/rfcs/0001-architecture.md)

当前进度：**M1 骨架**。手机能通过 SSH 连上主机，并执行 `herdr remote-api-bridge --check`。Android 和 iOS（iPhone、iPad）都已接入同一个 Rust 内核。

## 仓库结构

```
SPEC.md               工程规范
Cargo.toml            Rust workspace 根
crates/               Rust 内核，所有平台共用
├── kite-ssh/         SSH 连接、ed25519 密钥、严格的主机密钥校验、执行命令
├── kite-herdr/       在主机上查找 herdr 并检查 API 桥接
├── kite-app/         应用层：编排上面这些 crate
├── kite-ffi/         唯一对外的库：通过 UniFFI 导出给 Kotlin / Swift
└── uniffi-bindgen/   生成绑定代码的命令行工具
android/              Android 应用（Kotlin + Jetpack Compose）
ios/                  iOS 应用（SwiftUI，iPhone 和 iPad 通用，XcodeGen 工程）
docs/rfcs/            设计提案（RFC）
```

## Rust 内核

需要 Rust stable。测试包含一个进程内 SSH 服务端的端到端测试：

```bash
cargo test --workspace
```

## Android

环境要求：

- Android 编译目标：`rustup target add aarch64-linux-android`
- [cargo-ndk](https://github.com/bbqsrc/cargo-ndk)：`cargo install cargo-ndk`
- JDK 17 或更新版本
- Android SDK（compileSdk 36）和 NDK（版本见 `android/app/build.gradle.kts` 中的 `ndkVersion`），并设置 `ANDROID_HOME`

编译调试版 APK。Gradle 会先用 cargo-ndk 编译 Rust 内核，再生成 Kotlin 绑定：

```bash
cd android && ./gradlew :app:assembleDebug
```

安装到已连接的手机或模拟器：

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

默认只为 `arm64-v8a` 编译 Rust 内核。要在 x86_64 模拟器上运行，先执行 `rustup target add x86_64-linux-android`，再在 `android/gradle.properties` 里把 `kite.rustAbis` 改成 `arm64-v8a,x86_64`。

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
xcodebuild -project ios/Kite.xcodeproj -target Kite -sdk iphonesimulator CODE_SIGNING_ALLOWED=NO build
```

iPhone 和 iPad 的布局按窗口宽度切换，不按设备型号切换：iPad 在分屏或侧拉的窄窗口里用的是 iPhone 的单列布局。规则见 [RFC 0002](docs/rfcs/0002-ios.md)。

## 使用 M1

1. 打开 App，它会在本机生成一把 ed25519 密钥。私钥不会离开设备：Android 上用 Keystore 加密保存，iOS 上存进只在本机可用的 Keychain。
2. 点「复制公钥」，把公钥加到主机上对应用户的 `~/.ssh/authorized_keys`。
3. 填写主机地址、端口和用户名，点「连接并检查 herdr」。
4. 第一次连接时，App 会显示主机密钥指纹。在主机上运行 `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` 核对一致后，点「信任并连接」。
5. 看到「herdr 可用」和 `herdr-api-bridge-v1` 就说明 M1 通了。

App 会在 `command -v herdr`、登录 shell 的 PATH 和常见安装位置（`~/.local/bin`、Homebrew、Nix 等）里查找 herdr。装在别处时，在「herdr 路径」里填写完整路径。

在 Android 模拟器里，`10.0.2.2` 指向运行模拟器的这台电脑。iOS 模拟器和 Mac 共用网络，直接用 `127.0.0.1` 即可。

## 许可证

[GPL-3.0](LICENSE)。
