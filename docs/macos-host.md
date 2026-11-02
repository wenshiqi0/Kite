# Mac 本机服务

Kite 的“本机服务”窗口让手机和其他电脑通过 SSH 连接此 Mac。它与客户端工作区独立：Mac 可以同时连接 Linux，后台节点仍可接受其他设备；关闭窗口、退出 Kite 或停用外连不停止本机 agent。

## 开始

1. 打开 Kite 工具栏的 **本机服务**（也可从菜单栏纸飞机进入）。安装 Herdr 0.9.5+；聊天使用已安装、已登录的 Codex 0.166.0+，或已安装的 Claude。组件详情提供缺失依赖的官方安装入口。Bun 和 `kite-node` 随 Mac 应用交付。
2. 点击 **安装并启用本机节点**。用户级 launchd 节点和 Herdr 登录后启动；已运行的外部 Herdr 被复用。无需管理员权限。状态查询和普通启动不读取 Kite Keychain 私钥，也不触发其授权流程。
3. 在系统设置 → 通用 → 共享中确认 **远程登录** 和允许访问的用户。使用面板给出的实际机器名、用户名、端口；同一局域网使用 `.local` 名称，不要求 Tailscale。
4. 在另一台设备的 Kite 设置中复制该设备的 SSH 公钥，加入 Mac 用户的 `~/.ssh/authorized_keys`（目录 0700、文件 0600）。保留已有条目；不复制其他设备私钥。通过独立可信渠道核对 Mac SSH 主机公钥后再连接。
5. 需要桌面时安装官方 Sunshine Mac 应用，点击 **启动 Sunshine**。首次按 macOS 提示授予 Sunshine 屏幕录制、辅助功能及需要的音频权限；首次 Web 管理设置和 PIN 配对在 Sunshine 中完成。Kite 不重置系统授权，不覆盖已有 Sunshine 身份、配置或配对。

主机角色不自动启用系统 SSH、不自动授权新设备，也不修改用户全局 Codex 配置或凭据。Codex 安装在专用目录时可使用 `~/.local/lib/kite/macos/codex/node_modules/.bin`；安装器将其与常用 Homebrew、Bun、Cargo、NVM 路径放入后台服务的明确 PATH。Mac 上低于 0.166 的 Codex 不在可用 provider 目录显示。

## 启停和更新

本机面板分别显示会话服务、Herdr、SSH 和 Sunshine；串流服务在线仅证明管理接口在线，不能替代画面、输入和声音验收。状态窗口打开时 5 秒刷新，后台菜单状态 15 秒刷新；失败清空旧报告并保留操作错误。

**停用节点**只停用 Node，不终止 Herdr、Sunshine 或 provider。**更新并重启节点**安装新的私有组件代并重启 Node；客户端需要重连，后台任务、节点 ID、会话目录和 provider 历史保留。Mac 当前不声明 systemd 的无缝连接交接能力。

```sh
# 应用内相同的结构化管理入口
"$HOME/Applications/Kite.app/Contents/Resources/host/Kite Host.app/Contents/MacOS/kite-node" host inspect
"$HOME/Applications/Kite.app/Contents/Resources/host/Kite Host.app/Contents/MacOS/kite-node" host install
"$HOME/Applications/Kite.app/Contents/Resources/host/Kite Host.app/Contents/MacOS/kite-node" host enable
"$HOME/Applications/Kite.app/Contents/Resources/host/Kite Host.app/Contents/MacOS/kite-node" host update
"$HOME/.local/bin/kite-node" status
"$HOME/.local/bin/kite-node" mcp
```

私有配置在 `~/.config/kite/node.json`，IPC 在 `~/.local/state/kite/node/host.sock`，组件代与私有日志在 `~/.local/lib/kite/macos/`。后台以固定位置、固定 bundle ID 和证书要求的 `~/.local/lib/kite/macos/Kite Host.app` 运行，文件访问授权不会绑定到每次变化的摘要目录。首次访问受系统保护的文件时，授权对象为 **Kite Host**；状态刷新不触发权限申请。安装器串行化所有安装/启停操作。GUI 关闭不结束 launchd；登录自动启动不等于系统重启前的原进程可以存活。

## 能力范围

Node API 1、backend 5、harness 2、会话目录及原始历史合同保持不变。Darwin 的 PID/UID/微秒启动时间校验防止串用已退出进程；SIP 保护 shell 不提供环境变量时，从已验证的 Herdr pane/server 接口取得 hook 上下文。Mac 的新 Codex 会话在接入终端前创建并命名，确保空会话已经落成可恢复的 legacy rollout；不会为初始化发送模型提示。

Mac 暂不支持 Linux cgroup 共享内存硬限额、进程暂停、X11 agent 桌面操作。Node 返回 `resourceApi=0`，限额写入明确拒绝，provider `pause=false`，MCP 不声明 X11 工具。Sunshine 的 Moonlight 视频/输入服务独立于这些能力。

Android **Alpha 67 及以上**增加 Mac 捕获尺寸探测：通过 SSH 调用 Mac Node，由实际 Sunshine 监听进程和选定 CGDisplayID 取得像素尺寸。不会套用 Linux X11、固定 720p 或 SSH 登录默认显示器。首次桌面授权、配对、手机安装和物理音频分别验收。

参见 [本轮验收](product/2026-11-03/macos-host/acceptance.md)、[RFC 0029](rfcs/0029-macos-host.md) 和 [官方 Sunshine Mac 支持说明](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2getting__started.html)。
