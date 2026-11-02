# Harness 能力对齐验收（2026-11-02）

本轮修复 DSH 模型、思考强度、原生视觉与运行中补充信息，接入 Linux Ply，并把客户端 harness 改成按主机可用性生成的下拉框。

- DSH 使用 ACP 与原生 LLM 目录，真实 MiMo Pro/high 和 Flash/off 图片识别通过；重启恢复同一个原生 session 并保留上下文。
- DSH 原生 steering 在模型运行中接收补充信息，后续回复包含只在补充中提供的随机校验码；原生消息不重复提交。
- Linux Ply 的 Pro/high 和 Flash/off 原生图片输入、模型切换和私有上下文恢复通过；图片与提示词放在同一用户消息中。目录与已鉴权模型服务取交集。
- Ply 真实 read 工具需要当前客户端的明确批准，公开工具过程可见；原生 MCP 取消后进程仍可响应。
- Android 16 原生组件测试通过：已验证 harness 下拉选择、不可用项隐藏、原生模型/effort 切换、无思考模型隐藏 effort、初始化期间禁止发送。
- Python 64 个测试通过（含 11 个 harness 协议测试、运行中补充信息顺序与唯一 owner）；Rust 领域与 FFI 测试、Clippy 和 Android JVM/lint 检查通过。Ply/DSH 资源 owner 与后代进程有隔离回归测试；最终合并版本已通过发布 CI。

[结构化真实调用证据](native-results.json) 只保留本轮自有会话的能力与结果，不包含密钥、原始消息、隐藏推理或用户的会话身份。DeepSeek Flash 的选项读取 DSH 原生目录；本机真实供应商调用使用已配置且可用的 MiMo 路由，不冒充 DeepSeek 供应商实测。

内部配置按需读取 [DSH](../../../development/dsh.md) / [Ply](../../../development/ply.md)，不扩充日常 Agent 工具说明。

## 最终交付

[Alpha 50](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.50)，versionCode **52**，源提交 `fc9d0df3635a9c797be5469d96776f4d5e9c4c10`。公开 APK 的校验和、稳定签名、主包/兼容包载荷一致性和 16KiB 对齐均通过。[公开包记录](public-verification.json)。

匹配的 Linux Node 已以兼容交接方式安装，报告 `harnessApi=2`、`resourceApi=1` 和 ready；Codex、Claude、DSH、Ply 都通过当前主机目录检查，身份、现有环境和 Herdr/Sunshine PID 保留。[主机记录](host-verification.json)。

对正式安装的主机分别新建 DSH/Ply 自有会话，验证唯一 owner、两个可用模型、图像模态、思考选项以及共享内存预算中的实际进程归属。验收会话仅做只读准备，已归档并停止其自有空闲进程。[实际启动记录](owned-owner-results.json)。

候选 47 未发布；候选 48/49 的正式部署验收分别暴露环境变量白名单、未绑定草稿导致资源准入失败的问题，均已在最终包修正并增加回归测试。真实模型内容验证与原生组件测试已完成；不声称做过实体手机验收。

Ply 原生宿主接口源提交为 [1d462fb](https://github.com/wenshiqi0/Ply/commit/1d462fb)，Linux 原生运行时已安装，配套 npm 依赖保留在对应源码工作区。
