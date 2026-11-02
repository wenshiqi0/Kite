# RFC 0025：Ply ACP 插件接入

状态：已实现，待发布 · 2026-11-02

Ply 将 MCP 宿主扩展替换为独立 ACP v1 插件。Kite 同步使用 `ply acp`，不保留 MCP 协议回退。

- 通过 ACP initialize 验证 agentInfo、协议版本、图片能力及模型目录元数据。
- 使用 session/new、session/load、session/prompt、session/set_config_option、session/cancel。
- 使用 session/update 投影公开回复和工具事件，使用 session/request_permission 显式回复一次授权。
- 恢复引用改存原生 sessionId，Ply 私有上下文留在 Ply 的会话存储中。旧 MCP 快照不删除，
  不作为 ACP 恢复输入；旧会话原始历史仍保留，继续运行旧 owner 或另建 ACP 会话。
- 默认命令为 `ply acp --approval-mode ask --model ...`。现有自定义 MCP argv 需显式迁移，
  不根据旧配置猜测或静默改写宿主命令。
- 不修改 Core/FFI、原生客户端协议或其他 provider 生命周期。

验证：Bun 类型/lint/测试、ACP 双向权限/取消/恢复夹具，以及与本次 Ply 二进制的本地端到端验证。

验证记录（2026-11-02，Linux）：`bun run check` 的类型、边界、lint 和 45 项测试全部通过。
新适配器已连接 Ply `d763d6d` 二进制与本地 HTTP 模型验证首轮及重启后的上下文继续。
测试专用命令固定当前构建入口，避免宿主的已安装适配器路径污染夹具；未发起真实供应商请求，未替换正在运行的 owner。
