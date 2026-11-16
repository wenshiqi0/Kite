# Windows 客户端

Windows x64 原生客户端使用 C# / .NET 10 WPF 与共享 Rust Core。支持添加、编辑和移除远程主机、独立多主机连接与自动恢复、会话新建/继续/归档/删除、流式聊天、审批与问题、模型/思考强度/权限设置、Skills、附件、主机资源和远程桌面。

主机管理指客户端管理远程目标。Windows 本机不承担主机角色，不安装或启动 Kite Node、Herdr、Sunshine、Bun 或 provider 服务。远程目标仍按[主机安装文档](backend-setup.md)配置。

## 使用

正式交付包从 [GitHub Releases](https://github.com/wenshiqi0/Kite/releases) 下载版本化的 `kite-X.Y.Z-windows-x64.zip`；本地测试包仍叫 `Kite-windows-x64.zip`。完整解压后运行 `Kite/Kite.exe`，保留旁边的 DLL 和运行时文件。包自带 .NET 运行时，无需安装开发工具或以管理员运行。建议 Windows 11 或仍受支持的 Windows 10 x64；Windows N 需要系统 Media Feature Pack 才能解码 H.264。

点击“设备公钥”，把公钥加入主机的 `~/.ssh/authorized_keys`，然后添加并连接主机。首次连接明确确认指纹，变化的指纹不自动通过。各主机独立连接和恢复，切换主机保留连接，桌面媒体随目标切换关闭。

桌面初始为只看，点击“进入操作”发送键鼠；失焦、目标切换与关闭释放输入。全屏时 `Ctrl+Escape` 返回普通窗口。声音默认开启，可以静音；文本剪贴板默认关闭，仅在当前操作目标内同步。聊天用 `Ctrl+Enter` 发送，普通 Enter 换行。

数据位于 `%LOCALAPPDATA%\Kite\Windows`。设备 SSH 密钥及 Sunshine 身份经当前 Windows 用户的 DPAPI 加密，独立于其他设备；损坏或解密失败报错，不自动重建设备身份。退出只清理客户端外连，不停止远程任务。

## 在 Linux 交叉编译

需要 Rust stable、Bun 1.3.14+、.NET 10 SDK、CMake 3.20+、Ninja 和 POSIX 线程版 MinGW-w64 x64 编译器。Debian/Ubuntu 可以安装：

```sh
sudo apt-get install gcc-mingw-w64-x86-64-posix g++-mingw-w64-x86-64-posix cmake ninja-build
rustup target add x86_64-pc-windows-gnu
```

当前 Debian 13 开发机没有免密码 sudo，也可以使用无 root 引导脚本。它将 MinGW、CMake 和经微软发布元数据 SHA-512 校验的 .NET SDK 10.0.404 安装到用户缓存，不修改系统或 shell 配置；Rust/Bun 须预先安装：

```sh
windows/scripts/bootstrap-linux.sh
```

在仓库根目录执行：

```sh
git submodule update --init --recursive
windows/scripts/build-linux.sh
```

输出为 `windows/dist/Kite-windows-x64.zip` 及 `.zip.sha256`。脚本先运行聊天增量与视口测试，交叉编译 Rust DLL 和 Windows 媒体 DLL，再使用 .NET [Windows targeting](https://learn.microsoft.com/en-us/dotnet/core/project-sdk/msbuild-props#enablewindowstargeting) 发布 self-contained win-x64 程序。压缩前检查程序和原生 DLL 的 PE 架构、导出、依赖和客户端包边界，清单记录每个文件的 SHA-256 与源码提交。交叉生成的 apphost 显式设置 GUI 子系统，避免启动控制台窗口。

依赖固定在 Cargo.lock、Moonlight submodule 和 libopus 1.5.2 发布包摘要中。`KITE_WINDOWS_TOOLS_DIR` 改变工具缓存位置，`CARGO_BUILD_JOBS` 控制编译并发。Windows 包目前不带 Authenticode 代码签名；版本和来源记录在包内清单中。正式发版通过 [GitHub 发布流程](releases.md) 构建并验证，同一 Release 同时提供 Android 和 Linux 产物。

## 验证

Linux 可运行：

```sh
cargo test --locked -p kite-app -p kite-store -p kite-stream -p kite-ffi --features kite-ffi/windows-client
dotnet run --project windows/Tests/Kite.LogicTests.csproj -c Release
dotnet build windows/Kite/Kite.csproj -c Release -p:TreatWarningsAsErrors=true
```

`Windows Client` CI 在 Linux 构建 ZIP，再将该 ZIP 原封不动送到 Windows runner，验证 DPAPI、身份持久化、主机增删改、加载与快照、Media Foundation 实际 H.264 解码及 WPF 启动渲染。`Kite.exe --self-test` 使用临时目录，不连接真实主机；`--ui-smoke <PNG路径>` 渲染空工作区并退出。两种测试均不读取用户数据。

离线测试和 Linux 编译不等于 Windows 实机跨机聊天、IME、键鼠、音频设备或公网媒体验收。已完成范围记录在[验收记录](product/2026-11-15/windows-client/acceptance.md)。

`Windows Native Media` 工作流可独立交叉编译并验证系统解码器。`Windows Client` 的手动触发还可用 `artifact_run_id` 只复测已有 Linux 包；它不会重建当前源码，验证记录应以该包的 build-manifest 为准。

## GitHub Release

Windows 版本沿用 `android/version.properties` 中的统一正式版或 Alpha 版本号。常规发版使用 GitHub 的 `Publish Release` 入口；以下 Windows-only CLI 保留用于独立发布。源码合并到 `main` 后，通过 `python3 android/scripts/release-alpha.py --windows --notes-file <发布说明>` 发布；注解标签明确选择 Windows。只有该标签的 Linux 构建和 Windows 运行检查都通过，CI 才发布 Windows ZIP、逐文件摘要清单及 `SHA256SUMS-windows.txt`。完整流程见[发布文档](releases.md#windows-only-releases)。
