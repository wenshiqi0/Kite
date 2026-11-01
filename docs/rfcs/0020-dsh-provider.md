# RFC 0020：可用 provider 目录与 DSH 托管会话

> 状态：已实现（Android/Linux Alpha 43） · 2026-11-02
> 相关：RFC 0007、0013、0014、0018；内部实施规范见 [Harness 接入](../development/harness-integration.md)。

## 目标与边界

用户要求接入 DeepSeek Harness，客户端简称 DSH，并通过主机环境变量启用：后端不能使用时不在新建会话中展示。Ply 因当前运行时仅支持 macOS、目标 Mac 不可连接，按用户最新要求暂缓。

沿用 Rust 领域/应用状态、SSH 与私有 Node 桥接、同次生成的 FFI 和原生展示边界。新增 DSH SDK stdio 协议适配，不改变 crate 依赖，不让 Android 直接解释 DSH 事件，不要求 Herdr 支持新的 agent kind。

## 设计

- `kite-agent::providers` 定义可用 provider 的操作元数据，Core 查询主机后发布 `Snapshot.providers`，FFI 映射 `ProviderInfo`。Android 从该列表生成新建选项；未知保存记录仍保留原 ID 和历史。
- DSH 必须显式设置 `KITE_ENABLE_DSH=1`。主机验证命令、凭据、模型服务的只读健康查询与真实 DSH SDK `initialize`，成功才发布 DSH。模型路由与命令可通过对应环境变量指定；探测不提交模型 turn，缓存最多 20 秒且按配置隔离。
- 主机适配器使用独立的用户私有 owner、socket、状态和历史。`source=kite:harness` 区分 Kite 持久逻辑会话与 Herdr/native 会话；客户端断开不停止模型，单会话恢复锁防止重复 owner。原生 SDK 子进程只由其 owner 管理。
- DSH `session/prompt` 与 `session.event` / `session.status` 转换为现有聊天命令的内部规范化子集与消息投影。Codex 方法名仅在 Kite 内部适配边界复用，不向 DSH 发送 Codex RPC。
- DSH SDK 当前创建新进程后不提供 SDK resume 方法。保持 owner 时连续使用原 SDK 会话；owner 正常结束后的恢复创建新 SDK 会话，并在首次显式提交时提供已保存的公开对话上下文。Kite 逻辑会话 ID 与历史保留，不宣称恢复原 SDK 内存或重放旧工具。中断时有可能已执行工具的 turn 不自动重发。
- 本阶段 DSH 支持文本、公开回复、工具调用/结果、历史、归档和恢复。模型由主机配置固定；附件、Skill 选择、额度、进程暂停、真实终端及结构化审批/问题未接入，客户端按能力关闭入口。DSH 原有策略保持原样，不加入自动批准或 bypass 参数。
- 适配器与 Node 按源码摘要固定并一起发布。Node 返回 `harnessApi=1`，匹配客户端检查该能力；部署保持已有身份、会话、Herdr/Sunshine 和外部 owner。环境文件只在主机保存，不随包或快照传输。

## 验收

验证关闭开关、缺少凭据、模型服务失败、错误 SDK 身份和缺失命令时隐藏 DSH；验证真实子进程的连续 turn、工具结果关联、重复恢复唯一 owner、跨会话拒绝和历史权限。Rust/FFI 与 Android 检查之外，真实 DSH 模型调用、原生界面展示及匹配主机部署分别记录，不能混为同一种证据。

## SPEC 影响

增加按目标主机可用 provider 目录驱动客户端的规则；记录 DSH 的适配与持久会话语义。既有命令、状态和媒体三条通道的机制保持不变。
