# RFC 0018：保留连接的 Linux Node 升级

> 状态：已实现（源码、隔离与本机部署验收，尚未发布） · 2026-11-01

用户要求升级时保留运行中的 agent 和已连接客户端。兼容面限定为 Node API 1、backend 协议 5、现有主机/身份/会话/通知存储；不新增旧版协议回退，不自动重发消息、审批、附件或输入。首次从没有交接能力的旧 Node 迁移不能追溯保留旧连接，必须单独显式执行维护迁移。

## 设计

- `kite-node.socket` 持有私有 Unix listener，服务通过 `StandardInput=socket` 继承它。已接受的连接仍由原代进程处理。
- 安装器调用结构化 `upgrade --binary PATH`，不重启支持交接的服务。旧代启动新二进制，继承同一个 listener；新代准备好配置/适配器/存储后，通过私有双向管道报告 ready 并等待提交。
- 通知扫描、推送与 push 注册在交接前通过应用层读写锁排空，避免两代覆盖通知游标。准备失败恢复旧代观察任务；新代提交前不启动观察任务。
- 新代通过短暂启动器创建，启动器退出后由 systemd user manager 收养。必须先完成进程身份核验和单独 prepare 握手，再准备配置；候选二进制需声明 `generationLaunch=1`。不能把仍由旧代作为父进程的新代直接设置成 MAINPID，否则旧代长期持有连接时，新代意外退出可能不被监管器检测到。
- `systemd-notify` 同步确认 MAINPID 交接后才激活新代。旧代停止 accept，等待已有连接自然结束；不因升级期限杀死旧连接，不重放连接数据。新代由同一个用户服务监督。
- 每代使用按源码摘要固定的 Python 适配器目录，旧连接不会读到被覆盖的新版脚本；本阶段不自动清理仍可能被旧代使用的目录。
- GUI 显示中的面板和正在进行的操作继续运行；有托盘且界面隐藏、操作完成时才执行新 launcher。无托盘时正常退出后下次启动使用新版本。安装器不强制退出管理界面。

保持 Rust/原生职责、crate 依赖与客户端 wire 格式不变。运行时与交接在 `kite-app::host_node` 内，安装器只编排 CLI/systemd，GUI 只管理自身展示生命周期。MCP 不暴露升级工具。

## 失败与验证

不接受 API/backend 不匹配的候选程序。准备或确认失败时保持旧代监听；MAINPID 已更换时先回退主管 PID，再结束失败候选。升级后核对服务 MainPID、监听端返回 PID 与二进制摘要。普通 restart、崩溃、主机重启或协议迁移不属于无缝升级。

隔离验收包括真实用户 systemd socket/service、连续双向流、在途一次性写入、升级后新连接、候选失败回退、旧代排空退出、观察任务单写者交接，以及旧代连接仍存活时新代被终止后自动恢复。持续字节流不能替代实际模型生成/手机视频验收；未执行范围须明确记录。

依据：[systemd socket activation](https://www.freedesktop.org/software/systemd/man/latest/systemd.socket.html)、[systemd-notify MAINPID 与同步确认](https://www.freedesktop.org/software/systemd/man/latest/systemd-notify.html)。
