# RFC 0027：macOS 原生远程客户端

> 状态：已采纳 · 2026-11-02

用户要求先实现 Mac 客户端，因此调整 RFC 0015 的平台推进顺序；Linux 客户端和 macOS 主机适配仍保留为独立阶段。

## 界面与核心

新增 `macos/` XcodeGen 工程，最低 macOS 14，SwiftUI/AppKit 原生界面。机器、会话、完整聊天、问题/审批、模型/思考强度及附件使用现有 `KiteCore` 命令和版本快照。聊天正文通过 `conversation_since` 的增量镜像展示，滚动只影响本地渲染。客户端不启动或停止本机 Herdr、Sunshine、Kite Node 或 provider。

Mac 使用独立的应用支持目录和本机 Keychain 设备身份，不导入其他设备的私钥，不改变现有存储、主机协议或 Android/iOS FFI。绑定和静态库同次生成；无历史 Mac 数据需要迁移。

## Apple 媒体通道

按 SPEC 4.4 在 `kite-ffi::apple` 新增 C ABI。原生注册回调及显式 retain/release 所有权，Rust 返回不透明整数 handle。注册表与正在运行的串流各持有 sink；移除注册表后，已在途的回调仍有有效上下文，最后一个 sink 释放时才调用原生 release。

H.264 和 Opus 保持现有 Moonlight 传输。视频回调只复制到有界预分配槽，不等待界面线程；VideoToolbox 解码、Metal 展示与原生音频播放在 Swift 中实现。帧完成计数与错误使用媒体 C ABI 回报，媒体数据不进入 UniFFI。Apple 平台条件编译只出现在 FFI。

输入经同一 C ABI 与 sink 的 `InputSender` 发送，Rust 同时检查操作状态。默认只看；固定 16ms 合并指针/滚轮、200ms 合并已提交文字，按键/按钮作有序屏障。坐标和显示共享等比视口变换。切换只看、窗口失焦、切换目标或关闭窗口立即取消待发输入并释放按键。IME 只发送已提交文本；操作开关与 agent 暂停互不影响。

剪贴板默认为关，原生读写 NSPasteboard，Rust 继续验证主机身份及操作状态。异步用户操作捕获明确的主机和会话上下文，目标切换后不提交到新的目标。

## 验证

分别记录原生构建、离线媒体/输入/生命周期回归、真实跨机聊天与视频/输入。只有实际通过的范围才标为通过；离线夹具、编码包计数、模型模拟应明确区分。签名、公证、Intel Mac、真实音频设备与外网质量各自验收，不由本机 Apple Silicon 编译推出。
