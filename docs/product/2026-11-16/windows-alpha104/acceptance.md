# Windows Alpha 104 发布记录

2026-11-16，Windows 客户端与 Alpha 103 及最新会话删除修复均已在 GitHub `main`。发布标签 `v0.2.0-alpha.104` 指向 `16f99960ae8b7289581aa8896628845d5f49a6ea`，版本号为 `0.2.0-alpha.104`；统一版本账目的 Android versionCode 为 106，本次只交付 Windows 包。

[公开 Release](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.104) 目标为 `main`，是已发布的 prerelease，不是草稿或 latest 稳定版。包含版本化 Windows x64 ZIP、来源/逐文件摘要清单和 `SHA256SUMS-windows.txt`。

[标签 Windows CI](https://github.com/wenshiqi0/Kite/actions/runs/43780567975) 与 [main Windows CI](https://github.com/wenshiqi0/Kite/actions/runs/43780568287) 均通过。标签 CI 先在 Linux 交叉编译，再在 Windows 检查 DPAPI、重启后的设备身份、主机增删改、实际 H.264 解码和 WPF 启动渲染，最后才创建并验证 Release。Android/Docker 标签任务按 Windows-only 注解跳过，常规 main 检查仍保留。

公开发布后，另行通过未附带登录凭据的公开 URL 下载三个附件，验证 ZIP/清单的 SHA-256，并核验 ZIP 中 252 个文件的摘要、干净源码提交、版本号及客户端角色。精确哈希、附件链接及构建记录见 [published-release.json](published-release.json)。本地合并后检查包括 118 项 Rust、123 项 TypeScript（另有 1 项 Darwin 测试在 Linux 跳过）、15 项发布回归，Clippy 与工作流检查均通过。

本次没有做真实 Windows 设备与主机之间的完整聊天/IME/键鼠、公网质量及物理音频设备验收，安装包仍是未签名 Alpha。

独立的 [Android main 检查](https://github.com/wenshiqi0/Kite/actions/runs/43780568416) 首次在未修改的 Linux 原生面板截图夹具处失败，已请求重试；它不是本次 Windows Release 的发布流水线，也不代表已验证 Android 安装包。
