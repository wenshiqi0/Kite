# RFC 0030：网络 ACP 与跨机器会话工作区

状态：已实现（WebSocket 首阶段） · 2026-11-04

## 目标

本机与 mini 的会话能协作，用户从统一侧栏管理多台机器的会话；后续可显式把编译和构建任务交给 mini。此次先提供可验证的网络会话入口、明确的目标身份和结果返回，不自动按负载迁移或重放任务。

## 统一侧栏

Rust 的工作区快照发布每台已保存机器的会话、能力、连接与选中信息，包括离线缓存。原生左栏在顶部一行显示紧凑搜索与右侧新会话加号，下方按机器分组展示会话。机器分组提供更多菜单进入机器配置及连接操作。搜索覆盖全部机器，列表身份使用主机 ID 与会话 ID；打开会话是原子切换机器与会话，不能先切换再对当前机器猜测目标。新会话明确选择目标机器及该机器验证可用的 provider。

## 网络 ACP

使用 ACP v1 JSON-RPC。官方稳定传输仍是 stdio；网络部分参考当前 [Streamable HTTP / WebSocket RFD](https://agentclientprotocol.com/rfds/streamable-http-websocket-transport)，首阶段实现 `/acp` 的 WebSocket 通道，并明确不声明已实现完整 HTTP/2 + SSE profile。

Kite 是网络 ACP agent：适配到已有主机会话与唯一 provider owner，不在网络层增加新的模型循环。支持初始化、会话索引、新建、`session/resume`、文本 prompt、进度、权限请求和取消；新建使用已验证的 Codex/Ply/DSH，已有 Codex/Ply/DSH 会话可恢复。此次 `loadSession=false`，不声明完整 ACP 历史重放；正常 Kite 历史读取仍走原主机连接。会话 ID 始终绑定目标主机，恢复复用原始会话与 owner，连接中断不停止后台工作，不自动重发消息、审批和构建。

默认监听回环地址，连接认证在 WebSocket upgrade 完成前执行；跨机器复用已信任 SSH 及端口转发，凭据和模型密钥保留在各自主机。限制消息、连接、排队输出、超时和请求并发；浏览器 Origin、无认证连接、跨会话响应和非法工作目录明确拒绝。允许的工作目录显式配置，编译在目标机器的目录及其资源预算内执行。网络层不提供任意 shell RPC。

Rust Node CLI 管入口和目标参数；现有 TypeScript runtime 提供网络机制，provider 适配负责 ACP 与既有 owner 协议转换。沿用当前 crate/package 边界，网络 gateway 不取得外部 owner 的生命周期所有权。

## 验收

- 双机器会话同屏、相同会话 ID 隔离、跨机器搜索、新建目标选择、打开会话的原子目标、归档排序与选中保留。
- 网络握手、认证拒绝、双向权限、取消、断线不重放、恢复原 owner、大小及并发边界。
- 本机通过网络 ACP 驱动 mini 的自有会话执行一个有明确源码版本的构建/校验任务，收到实际进度和结果；验证过程不借夹具声明真实模型支持。
- CLI/协议验证与原生 UI 运行验收分别记录；未验证的 provider 和 HTTP profile 不声明支持。
