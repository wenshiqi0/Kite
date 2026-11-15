# Windows 客户端验证记录

2026-11-15，`feat/windows-client`。在 Debian 13 x64 Linux 上开发与交叉构建，不使用用户真实会话做夹具。

- 已完成：原生 WPF 界面和 MinGW 媒体 DLL 的 Linux 编译；聊天增量原子应用、旧版本拒绝、跨目标清理和视口坐标测试。
- 已完成：共享客户端 Rust 回归 110 项（应用 88、边界 6、存储 4、串流 12）。媒体回归覆盖观看门禁、释放顺序、旧代帧及在途回调所有权。
- 整包：Linux 构建、PE/DLL 依赖校验和 ZIP SHA-256 完成后记录。
- Windows：CI 将运行同一 Linux 产物的 DPAPI/身份/主机管理、系统 H.264 解码与 WPF 渲染验证，结果完成后补充。
- 未验收：真实 Windows 设备与真实主机的端到端聊天、桌面输入/IME、物理音频设备、公网延迟；Windows 代码签名和 ARM64 不在本次范围。

Windows 本机只运行客户端，不包含 Node、Herdr、Sunshine 或 Bun 可执行文件。远程主机配置、服务和既有任务没有为测试而修改。
