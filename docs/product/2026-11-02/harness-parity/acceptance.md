# Harness 能力对齐验收（2026-11-02）

本轮修复 DSH 模型、思考强度、原生视觉与运行中补充信息，接入 Linux Ply，并把客户端 harness 改成按主机可用性生成的下拉框。

- DSH 使用 ACP 与原生 LLM 目录，真实 MiMo Pro/high 和 Flash/off 图片识别通过；重启恢复同一个原生 session 并保留上下文。
- DSH 原生 steering 在模型运行中接收补充信息，后续回复包含只在补充中提供的随机校验码；原生消息不重复提交。
- Linux Ply 的 Pro/high 和 Flash/off 原生图片输入、模型切换和私有上下文恢复通过；图片与提示词放在同一用户消息中。目录与已鉴权模型服务取交集。
- Ply 真实 read 工具需要当前客户端的明确批准，公开工具过程可见；原生 MCP 取消后进程仍可响应。
- Android 16 原生组件测试通过：已验证 harness 下拉选择、不可用项隐藏、原生模型/effort 切换、无思考模型隐藏 effort、初始化期间禁止发送。
- Python 63 个测试通过（含 11 个 harness 协议测试、运行中补充信息顺序与唯一 owner）；Rust 领域与 FFI 测试、Clippy 和 Android JVM/lint 检查通过。Ply/DSH 资源 owner 与后代进程有隔离回归测试；最终合并版本继续由发布 CI 检查。

[结构化真实调用证据](native-results.json) 只保留本轮自有会话的能力与结果，不包含密钥、原始消息、隐藏推理或用户的会话身份。DeepSeek Flash 的选项读取 DSH 原生目录；本机真实供应商调用使用已配置且可用的 MiMo 路由，不冒充 DeepSeek 供应商实测。

内部配置按需读取 [DSH](../../../development/dsh.md) / [Ply](../../../development/ply.md)，不扩充日常 Agent 工具说明。
