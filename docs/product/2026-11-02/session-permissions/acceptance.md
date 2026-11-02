# 会话权限与审批修复

## 实现

统一实时 agent 清单用于审批应答，覆盖 Herdr 之外的 Ply/DSH owner。审批展示操作、完整命令、实际工作目录和参数，允许一次/拒绝保持原请求身份。模型浮层增加按原生能力显示的会话权限设置；Ply 有 ask/risk/bypass，Codex 有审批策略和文件访问范围，DSH 当前未提供权限参数。

设置按主机/会话保存，从下一条消息起生效，队列保留每条消息自己的选择，不自动批准待确认操作。Codex 选项受 configRequirements/read 限制，已有工作区根目录和网络参数保留。

## 本地验证

- Kite Rust workspace：166 项通过；Clippy all-targets 通过。
- Bun typecheck/lint/包边界及 47 项测试通过；新增原生权限选项、实际命令、无效值及排队策略用例。
- 本地 SSH 夹具调用真实 AppCore.respond_approval，覆盖 Ply/DSH/Codex 的允许、拒绝、过期和重复请求，证明 Herdr 不含托管 agent 时仍能应答。
- AppCore 设置保存后切换模型，再发送下一条消息，原生 turn/start 收到原值；非法枚举被拒绝。
- 对应 Ply 源码：bd59d958db0c668a1af95a38264adeb667df8c10。npm check 和完整 Rust workspace（包括 Node 兼容、PTY、ACP、真实工具门禁）通过；安装后的 ACP 握手与策略切换通过，未发起供应商模型 turn。

Android Debug/JNI/测试 APK、Lint 与 75 项 JVM 测试通过；Android 16 原生界面三项测试通过，覆盖完整命令卡片、文件读取/拒绝、模型思考选择以及模型浮层里的权限菜单和原生值回传。已查看 [命令审批](screens/readable-command-approval.png)、[文件审批](screens/readable-file-approval.png)、[权限菜单](screens/permission-policy-menu.png) 和 [权限选择](screens/permission-policy-selected.png) 截图。原生测试使用隔离只读 AVD 和 KVM 容器，未操作用户桌面或发起模型请求。最终公开发行与实体手机安装分别验证。x86 QA 库显式链接 NDK compiler-rt builtins 解决现有 CPU feature symbol 链接问题；ARM64 发布继续使用常规 CI 构建。

## 发布候选

Alpha 62 标签保留且未发布：发现缺少主线 Alpha 61 的 desktop 观察隔离改动后取消了构建。合并完整主线，仅版本文件发生冲突，使用 Alpha 63 / versionCode 65 解决；不会移动或覆盖 Alpha 62 标签。Android 功能代码和已验收的 FFI/原生组件保持一致。
