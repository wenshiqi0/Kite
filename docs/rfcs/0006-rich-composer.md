# RFC 0006：模型、思考强度与附件输入

> 状态：已实现（Android Alpha 6） · 2026-10-30

用户确认保留聊天历史、手机密钥、主机信任及配对；后端与 APK 同步升级，不维护旧聊天入口。herdr 0.9.5 仍负责 pane、终端和会话发现。Codex 聊天改用同一会话所有者的 app-server v2，不通过终端文本模拟模型设置或图片输入。

- 每个 Kite Codex pane 使用独立、持久、仅当前用户可访问的 Unix app-server socket。herdr 的 Codex TUI 与手机通过 SSH 连接同一服务器。不得同时用两个 app-server 恢复同一 thread，不改全局 Codex daemon 或认证配置。
- `backend/kite_codex.py` 是 Linux 主机辅助程序，依赖 Python 3 和 Codex 0.164.0。由 Rust 安装到用户目录；新 pane 启动前准备服务器。旧会话只在空闲且身份校验成功时由显式升级命令迁移，继续使用原 thread UUID。
- `kite-agent` 实现有界 JSONL RPC、模型目录、turn/start 和附件传输；`kite-app` 编排和持久化选择，`kite-ffi` 只映射类型。模型及可用思考强度来自 `model/list`，实际 turn 显式传 model/effort。
- 原生层选择文件并复制到私有缓存，只传路径和元数据；Rust 校验大小、流式上传、校验 SHA-256 后原子提交。单文件最多 25 MiB，每条最多 5 个。上传失败保留草稿，可移除或重试；不自动重放结果未知的消息发送。
- 图片使用原生 `localImage`，支持音频的模型使用 `localAudio`；普通文件和不支持原生音频的模型使用已上传文件路径供 agent 工具读取。界面明确区分音频附件与语音转文字，不宣称所有模型能直接听音频。
- 目录、设置、附件队列和进度由 Rust 快照持有，原生只保留平台文件选择与临时 UI 状态。聊天记录仍读取同一 provider 历史，附件元数据随用户消息保存；Codex 历史在主机投影时去掉内嵌图片字节，再进行有界传输，避免大图片挤掉用户消息。源历史文件不修改。
- 审批继续由已附着的 TUI 持有，Rust 读取实际菜单并发送对应按键；手机 RPC 客户端只提交命令，不接管审批所有权，不自动允许。未知交互转入真实终端。
- 菜单仅点击打开，禁用侧滑。只看/操作与 agent 暂停独立；精简桌面工具，缩放沿用源分辨率坐标映射。

协议依据：[Codex app-server](https://learn.chatgpt.com/docs/app-server) 与主机 Codex 0.164.0 生成的 JSON Schema。该接口仍属实验性质，当前仅用于 Alpha。

## Alpha 16 输入入口

输入框左侧加号只提供图片、文件和音频资源，桌面浮窗、上下分屏与全屏不再放入附件菜单。桌面入口留在顶栏更多菜单，布局由桌面控件切换。

当前 Kite 尚无 skills 目录查询或显式选择入口；`kite-agent::composer` 只发送文本、图片及受支持的音频输入。远端运行时的 skill 行为不作为客户端选择器的实现证明。此轮不增加 skills 接口或更改已有会话、附件、身份与信任的保存格式。
