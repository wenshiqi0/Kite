# Ply 主机配置

> 内部开发与排查资料；按需从 [Harness 接入规范](harness-integration.md) 进入。

Ply 已支持 Linux。Kite 使用 Ply 原生 MCP 宿主会话接口，要求安装具有 `--host-chat` 和 `plyAgent=1` 握手能力的版本。普通旧版 MCP 工具服务不会误判为可聊天后端。

主机私有环境（0600）：

```dotenv
KITE_ENABLE_PLY=1
KITE_PLY_MODEL=deepseek-v4-flash
DEEPSEEK_API_KEY=主机真实密钥
```

MiMo Token Plan 示例：

```dotenv
KITE_ENABLE_PLY=1
KITE_PLY_MODEL=mimo-v2.6-pro
KITE_PLY_PROVIDER=xiaomi-token-plan-cn
KITE_PLY_HEALTH_URL=https://token-plan-cn.xiaomimimo.com/v1/models
MIMO_API_KEY=主机真实密钥
```

`KITE_PLY_COMMAND` 可选 JSON argv，默认 `ply mcp --host-chat --approval-mode ask --model <KITE_PLY_MODEL>`，配置 MiMo provider 时附加 `--mimo-provider`。自定义命令必须保留宿主会话和审批能力，不应加入 bypass。密钥只来自对应环境变量，不放进 argv。

普通文件附件还需保留工作目录挂载，并只读挂载 Kite 上传目录。例如 command 中加入 `--mount disk:. --mount disk:/home/USER/.local/share/kite/uploads:ro`；`disk:.` 随该会话实际工作目录解析，上传目录用主机真实绝对路径。当前主机已配置该只读挂载，图片仍直接进入原生上下文。

开关、可执行文件、鉴权模型服务、目标模型及原生握手都通过后，Ply 才出现在客户端下拉框。原生模型目录决定名称、模态与思考选项；不复制 Codex 列表。MiMo 当前原生集成为 Pro/Flash、text/image、off/high；后续变化跟随原生目录。

宿主接口 `ply/agent/catalog`、`chat`、`state`、`approve` 不注册成模型工具。模型不能通过工具输入改写宿主配置或恢复快照。图片作为原生 image 内容输入；Ply 自己执行 agent loop、工具策略、审批与上下文管理。私有快照只用于重启后继续同一对话，隐藏推理不进入 Kite 公开历史。

运行中的补充输入由同一 owner 持久排队，前一轮结束后按顺序送入同一原生上下文，不直接报 busy，也不重复提交。

Kite 投影公开回复及原生工具事件，审批按请求身份应答；取消发送 MCP `notifications/cancelled`，不杀进程冒充正常完成。额度和显式 Skill 仍按真实接入状态隐藏。
