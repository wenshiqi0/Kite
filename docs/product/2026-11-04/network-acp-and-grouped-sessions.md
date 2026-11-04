# 网络 ACP 与统一会话侧栏

## 实现范围

Rust Core 发布所有已保存机器的会话目录、连接、能力、排序和选择，包含离线缓存。Android/Mac 左栏以机器为分类，顶部为紧凑搜索与右侧新会话加号；搜索和活动/归档过滤跨机器生效。机器更多菜单进入配置及连接操作。打开某个分类的会话通过同一 Rust 命令同时定位机器与会话；新建对话框显式选择机器和该机器公布的 provider。

网络入口为 ACP v1 WebSocket `/acp`，认证在升级前验证，Kite peer 客户端还会校验 nonce/HMAC endpoint proof。现有 owner 和 provider 持有实际模型循环、凭据、工具执行和持久上下文。网关只持有客户端代理，断开不会杀死 owner，也不重放输入。`session/resume` 继续原始会话；`loadSession=false`，不声称完整历史重放或 HTTP/2/SSE profile。

## 真实 mini 验证

执行机器为 `wen-mini`，编译目录是隔离的 `Kite-network-acp-qa` worktree。没有修改 mini 原始 Kite checkout。执行 provider 为其已配置的 Ply/MiMo；校验以真实工具输出为准，不以模型自述代替。

- 第一次启动发现 SSH 会话进程不在受管用户资源层级中，owner 准入被拒绝。将网关作为 systemd 用户服务运行后正常启动；实现加入提前检查和明确提示，保留现有预算约束。
- 第一轮在源码 `0297f158b158d8fa0da44a76f0d738cf421b4123` 实际执行一次 `bun run typecheck`，工具返回 `exit_code=0`、`stderr="$ tsc --noEmit"`。工具进度暴露了客户端把数组内容误当文本对象的解析问题，连接中断；修复并增加实际 socket/CLI 回归。
- 重连同一 Kite 会话 `6aa15b112a1542648ad9bae7afefcb25`，原 owner/thread `3b36f27a-95be-47db-aefd-69614b60aa76` 返回原轮次的命令、退出码及验收标记。该回忆请求未提供原标记，也没有再次执行构建。
- 在源码 `133805fbf6bce17cf47540d778b67901386412fc` 发起第二轮独立校验，CLI 收到实际工具开始、一次授权、工具完成与最终回复，`stopReason=end_turn`。原始工具结果确认命令仍是 `bun run typecheck`、退出码 0、未超时，源码版本在发起前由网关通过 Git 核对。

具体任务记录保存在本次私有验证制品目录；仓库不包含 token、provider key、设备身份或测试 SSH 授权。发布和运行范围以验收结论区分：编译通过、协议夹具、真实模型任务和原生界面运行是不同证据。

## 自动检查

- Rust 工作区测试通过，包含多机器离线目录、同名会话身份、无效目标不改变选中，以及全局排序/新机器默认与重建恢复。
- Bun 类型、边界、lint 和测试通过，覆盖 WS 认证/Origin、UTF-8、非法帧、权限隔离、取消、断线不重放、真实 socket CLI、工具数组内容与 MCP 在途取消。
- Android Release JVM tests、lint、Debug instrumentation 源码编译通过。Mac 原生测试及应用构建通过；截图/触控等设备运行结论单独记录。
