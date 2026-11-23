# RFC 0031：Windows 纯客户端与 Linux 交叉构建

> 状态：已实现（Linux 交叉构建和 Windows 离线验收；真实主机联调独立） · 2026-11-15

Windows x64 客户端提供远程主机管理、独立多主机连接、会话、结构化聊天、审批、问题、模型设置、附件和远程桌面。主机管理指客户端管理远程目标；Windows 本机不安装、运行或管理 Kite Node、Herdr、Sunshine 或 provider 服务。不增加 Windows 主机端。

## 原生界面与核心

`windows/Kite/` 使用 C# / .NET 10 WPF。现有 `kite-ffi` 增加 `windows-client` feature：通过进程内 C ABI 接收 UTF-8 JSON 命令，复用已有 `KiteCore` 的命令、快照类型和聊天增量。JSON 是边界序列化，不增加服务、监听端口或脚本运行时。阻塞 ABI 仅在原生工作线程调用，Tokio 运行时仍只在 `kite-app` 创建。整数 handle 注册表保证关闭期间的在途调用持有核心。C ABI 捕获 panic，返回结构化错误，分配和释放由同一 DLL 完成。

设备秘密由原生回调使用当前 Windows 用户的 DPAPI 加密保存，使用独立 LocalAppData 目录；密钥解密失败必须报错，不重新生成设备身份。主机指纹仍由用户明确确认。共享存储的原子替换和目录同步需要覆盖 Windows 语义，平台差异限文件系统基础设施。

## 媒体

将现有 Apple C 媒体适配器提取为平台中立的 C 媒体适配器，保留 Apple 导出符号，Windows 使用同一输入门禁、代次与注册所有权。视频和音频字节不经过 JSON。`windows/native/` 是 Windows 平台实现，使用系统 Media Foundation H.264 解码与原生 WPF 展示，固定版本的 libopus 解码及 waveOut 播放。传输回调只写入有界队列，不等待 UI；解码、播放和呈现独立。声音默认开启，可静音。

观看与操作明确切换；观看不发送输入。输入共享等比视口坐标，固定窗口合并移动/滚轮和已提交文字，按键作顺序屏障。失焦、切换目标、关闭和退出操作立即释放按键。剪贴板默认关闭，明确绑定目标与操作状态。异步聊天、审批与会话操作始终捕获主机及会话身份。

## 构建与验证

Linux 使用 MinGW-w64、Rust `x86_64-pc-windows-gnu` 和 .NET Windows targeting 编译，打包 self-contained win-x64 ZIP，不要求用户安装 .NET、Bun 或主机服务。构建脚本验证 PE 架构和分发依赖，并输出 SHA-256。保留 Windows CI 构建与原生媒体夹具回归；Linux 出包不等于 Windows 实机、跨机聊天、音频设备或公网串流验收。
