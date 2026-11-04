# RFC 0029：Mac 客户端内的本机主机角色

状态：已采纳，实施中 · 2026-11-03

按用户要求实现 RFC 0015 的 Mac 双角色。SwiftUI 增加独立本机服务窗口和菜单栏入口；服务政策、安装和状态仍由 `kite-app::host_node` 持有，界面只调用随应用交付的 `kite-node` 结构化 CLI。客户端纯使用不启用主机服务；退出应用不停止 launchd 用户节点或已有 provider。

沿用 Node API 1、backend 5、harness 2、现有 SSH 与私有 Unix IPC、会话存储和 provider 原始日志。Linux 配置和 systemd 机制保持原合同；Mac 新配置显式选择 launchctl，安装到用户私有目录，通过 LaunchAgents 登录启动。后台的签名应用 `Kite Host.app` 保持固定路径、bundle ID 和证书要求；组件代只用于准备/校验，不能让 TCC 把每次更新当作新摘要路径的命令行程序。已有设备密钥、信任和 Sunshine 配对不迁移、不重建。不新增公网控制协议，不自动启用系统 SSH、修改授权设备或系统隐私设置。

Mac 进程身份使用 libproc 的 UID、PID 和微秒启动时间，参数用 sysctl，工作目录用 libproc。SIP shell 不公开环境时，从配置好的 Herdr server/pane API 取得 hook 上下文；没有 `/proc` 回退。Codex owner、历史、附件、唯一恢复租约和归档生命周期沿用现有路径。Mac Codex 0.166+ 新会话通过 `thread/start` 显式指定 legacy 历史，再在同一连接中设置用户标题，确认 rollout 已持久化后启动恢复终端，不发送初始化模型 turn、不迁移现有历史。Mac 暂不声明 Linux cgroup 内存硬限额及 Linux 专用进程暂停能力：Node `resourceApi=0`、限额操作明确拒绝、provider `pause=false`；启动不经过 Linux cgroup 机制。不得把未落实的资源硬限额显示为已落实。

Herdr 使用官方 Mac 二进制，复用已运行实例；Sunshine 使用官方 Mac 发行版及既有身份目录，独立展示缺失、未启动、权限待处理和真实串流状态。系统 SSH 及屏幕录制/辅助功能授权需要系统设置操作，后台查询不得反复触发权限窗口。Sunshine 的首次权限与桌面/音频实际验收分开记录。

验收包括独立 launchd 节点、真实 Herdr 会话/聊天、SSH 远端访问、客户端同时外连 Linux、关闭窗口/退出客户端后服务保留、失败状态不冒充 ready。编译或 localhost 调用不冒充手机/另一机器及桌面输入验收。Mac 桌面 agent 的 X11 MCP 替代适配独立于 Sunshine 视频服务；尚不支持的动作明确报告。

依据：[Apple launchd](https://support.apple.com/guide/terminal/apdc6c1077b/mac)、[Apple proc_info](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info.h)、[Herdr Mac 安装](https://herdr.dev/tui/docs/install/)、[Sunshine Mac 安装与权限](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2getting__started.html)。
