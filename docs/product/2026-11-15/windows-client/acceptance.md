# Windows 客户端验证记录

2026-11-15，`feat/windows-client`。在 Debian 13 x64 Linux 上开发与交叉构建，不使用用户真实会话做夹具。

- 已完成：原生 WPF 界面和 MinGW 媒体 DLL 的 Linux 编译；聊天增量原子应用、旧版本拒绝、跨目标清理和视口坐标测试。
- 已完成：共享客户端 Rust 回归 111 项（应用 89、边界 6、存储 4、串流 12）。媒体回归覆盖观看门禁、释放顺序、旧代帧及在途回调所有权。
- 已完成：Linux 全量交叉构建与 self-contained ZIP；PE x64/GUI 子系统、DLL 导出、原生依赖和文件摘要校验。SDK 10.0.404、Rust 1.94.1、MinGW 14 POSIX；CI 另用 Ubuntu 24.04 的 MinGW 独立重建。
- 已完成：[独立原生媒体 CI](https://github.com/wenshiqi0/Kite/actions/runs/43665199909) 将 Linux 交叉编译的 DLL 送到 Windows Server 2025，系统 Media Foundation 实际输出 **3 帧，320×180，错误码 0**。显示尺寸按源桌面裁剪，保留编码表面的 NV12 行距。
- 已完成：[整包 CI](https://github.com/wenshiqi0/Kite/actions/runs/43666201827) 两个阶段全部通过。源码为 `a4f2992094b8844db4de32d89b4dfe261c81d66e`；Windows 执行的是 Linux 产出的原始 ZIP。DPAPI 往返、密文检查、拒绝损坏密钥、重启后身份/主机配置持久化、主机增删改、H.264 解码和 WPF 启动渲染均通过。原生主机移除命令独立于原有的清除信任命令，旧客户端契约保留。
- 未验收：真实 Windows 设备与真实主机的端到端聊天、桌面输入/IME、物理音频设备、公网延迟；Windows 代码签名和 ARM64 不在本次范围。

Windows 本机只运行客户端，不包含 Node、Herdr、Sunshine 或 Bun 可执行文件。远程主机配置、服务和既有任务没有为测试而修改。

[已验证的下载产物](https://github.com/wenshiqi0/Kite/actions/runs/43666201827/artifacts/14142293272) 内含 `dist/Kite-windows-x64.zip`；ZIP SHA-256 为 `3e9775ff0822dbb7b3e8b99d9f3141e900a8ebc32f97c623025999a2e158100a`。逐文件摘要与结果记录见 [validation.json](validation.json)。

[Windows 工作区启动截图](windows-startup.png) 来自隔离目录中的原生 WPF 渲染，并已查看；当前工具未提供 `cua_repl`，不把静态渲染记作真实桌面交互验收。
