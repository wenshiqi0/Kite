# RFC 0014：Linux Kite 桌面主机节点

> 状态：已实现（主机 Alpha 17；托盘 Alpha 18） · 2026-10-31

用户要求开始在 Linux 后端启用 Kite 桌面端。沿用已确认的历史、设备身份、SSH 信任、主机与 Sunshine 配对保留要求；APK 与后端同步升级。herdr-core 仍处于隔离评估，现有生产 Herdr TUI/provider owner 不迁移、不重启。

## 本阶段交付

- `kite-node` 常驻后台服务、Linux 桌面控制面板和可安装的用户级软件包。
- 新版 Android 的聊天、会话目录、历史和附件桥接通过 SSH 调用 `kite-node`，由常驻节点执行匹配版本的主机适配器；Herdr API 也由节点转发。不增加旧直连回退。
- 节点复用当前 Herdr/Sunshine 用户服务，显示与启动配置好的服务；不停止或替换外部拥有的服务。关闭控制面板不结束后台服务或 agent。
- MCP stdio 提供主机状态与会话目录读取。多机持久任务投递、模型间消息、完整桌面远程客户端及 herdr-core 生产迁移另行推进，不以本阶段冒充完成。

## 分层

新增 `crates/kite-node` 可执行入口，仅依赖 `kite-app`，负责命令参数和退出码。`kite-app::host_node` 在 `host-node` feature 下实现节点运行时、配置、服务策略、私有 IPC、桥接和 MCP；Tokio 运行时仍只在应用层创建。Android 的 `kite-ffi` 不开启这个 feature。Linux GTK 控制面板位于 `desktop/linux`，只调用节点 CLI、显示结构化状态和处理窗口事件，不保存领域状态或实现服务策略。

节点使用用户私有目录和 0600 Unix socket；外部访问沿用既有 SSH 身份验证，不新增公网监听或复制云端凭据。桥接先发有界 JSON 请求，再使用有界 stdout/stderr/stdin/exit 帧，保留原协议数据与退出状态，流式传输附件和压缩历史。禁止任意 shell 请求；只接受配置好的 Herdr 和已知适配器动作。节点只记录计数、版本和服务生命周期，不记录消息、文件内容或凭据。

当前 Python 适配器由节点安装为 `backend-v5.py`，继续使用原会话目录、注册表、provider 引用及恢复租约。旧文件属于已有安装，保持原样；新版 APK 不使用它们。服务探测、provider owner 恢复和真实聊天必须分别验证，单纯进程存在不算手机已切换。

MCP 按 [2025-11-25 stdio](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports) 和 [tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) 协议实现，stdout 仅输出 JSON-RPC。桌面 GUI 与 MCP 共享同一常驻节点。

## X11 常驻状态入口

控制面板增加常驻托盘入口，采用当前受支持的 Xfce/X11 桌面自带的 [GTK 3 StatusIcon](https://docs.gtk.org/gtk3/class.StatusIcon.html)。这是当前 X11 平台适配；该 API 已被上游废弃，不以它声明 Wayland/GNOME 托盘支持，也不增加其他托盘协议的兼容回退。没有可用托盘时显示控制面板，关闭该窗口退出 GUI，后台节点继续运行。

节点的 `status` 返回结构化 `health`（`ready` / `degraded`）：所有配置服务运行时为 `ready`，任一未运行或探测失败为 `degraded`。这反映本机服务运行状态，不表示手机已连接或模型/串流端到端可用。原生层负责启动中、节点不可达、图标与文案展示，不能把一次旧的成功状态保留成正常。托盘与可见窗口共用一个异步轮询器，3 秒刷新，状态调用超时 3 秒；命令执行期间显示处理中，完成后重新读取状态。

使用 [Gtk.Application / Gio.Application](https://docs.gtk.org/gio/class.Application.html) 保证同一桌面会话只保留一个 GUI 和托盘。关闭面板隐藏到可用托盘；显式“退出状态栏”只结束 GUI。安装 [XDG 登录自启动入口](https://specifications.freedesktop.org/autostart/latest/)，已有的用户自启动设置原样保留。GUI 自启动与节点 systemd 服务的生命周期分开。

## 验收

- 单节点实例、私有 IPC 权限、桥接 stdin/stdout/stderr/退出码和无旧回退。
- 原 agent PID/thread 引用、历史、目录、设备身份、信任及配对保留；生产服务不重启。
- Android 实际读取历史并发送受控会话消息，证实请求经过新节点；独立验证桌面视频和输入。
- Linux 控制面板实际渲染，关闭后节点仍存活；SSH 断连重连后仍可使用。
- MCP 初始化、工具发现和状态/会话读取；异常请求不会执行任意命令。

## SPEC 影响

增加 `kite-node → kite-app` 依赖边界和应用层可选主机运行时；Linux 原生面板只调用结构化 CLI。已有媒体与 FFI 边界保持不变。
