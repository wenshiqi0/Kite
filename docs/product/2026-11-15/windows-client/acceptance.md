# Windows 客户端验证记录

2026-11-15，`feat/windows-client`。在 Debian 13 x64 Linux 上开发与交叉构建，不使用用户真实会话做夹具。

- 已完成：原生 WPF 界面和 MinGW 媒体 DLL 的 Linux 编译；聊天增量原子应用、旧版本拒绝、跨目标清理和视口坐标测试。
- 已完成：共享客户端 Rust 回归 110 项（应用 88、边界 6、存储 4、串流 12）。媒体回归覆盖观看门禁、释放顺序、旧代帧及在途回调所有权。
- 已完成：Linux 全量交叉构建与 self-contained ZIP；PE x64/GUI 子系统、DLL 导出、原生依赖和文件摘要校验。SDK 10.0.404、Rust 1.94.1、MinGW 14 POSIX；CI 另用 Ubuntu 24.04 的 MinGW 独立重建。
- 已完成：[独立原生媒体 CI](https://github.com/wenshiqi0/Kite/actions/runs/43665199909) 将 Linux 交叉编译的 DLL 送到 Windows Server 2025，系统 Media Foundation 实际输出 **3 帧，320×180，错误码 0**。显示尺寸按源桌面裁剪，保留编码表面的 NV12 行距。
- Windows：CI 将运行同一 Linux 产物的 DPAPI/身份/主机管理、系统 H.264 解码与 WPF 渲染验证，结果完成后补充。
- 未验收：真实 Windows 设备与真实主机的端到端聊天、桌面输入/IME、物理音频设备、公网延迟；Windows 代码签名和 ARM64 不在本次范围。

Windows 本机只运行客户端，不包含 Node、Herdr、Sunshine 或 Bun 可执行文件。远程主机配置、服务和既有任务没有为测试而修改。
