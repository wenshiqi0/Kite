# DSH 主机配置与验证

> 内部配置/排查指南；接入目录见 [Harness 规范](harness-integration.md)。本轮接入 DSH，Ply 暂缓。

## 启用条件

安装匹配版本的 Kite Node 与 Android 客户端，并让主机上的 `dsh --profile sdk` 可以启动。DSH 默认关闭；只有开关打开、模型服务可用且 SDK 握手成功，DSH 才出现在新建会话列表。失败项不展示为不可点击的选项。现有保存会话仍可读取缓存。

在主机私有文件 `~/.config/kite/harness.env` 中配置，权限设为 `0600`。安装器生成的用户服务会读取该可选文件；也可将同名变量放在私有 `node.json.environment` 中供 Node 应用。环境文件在服务启动时读取；兼容交接时使用新配置的 environment。修改后应用匹配的 Node 配置，不重启外部 Herdr/Sunshine 或现有 provider。

```dotenv
KITE_ENABLE_DSH=1
KITE_DSH_PROVIDER=deepseek-official
KITE_DSH_MODEL=deepseek-v4-flash
DEEPSEEK_API_KEY=在主机配置真实密钥
```

| 变量 | 行为 |
| --- | --- |
| `KITE_ENABLE_DSH` | 仅精确值 `1` 启用；缺省或其他值关闭 |
| `KITE_DSH_COMMAND` | 可选，完整 JSON argv 数组；缺省 `["dsh","--profile","sdk"]`，不经 shell 解释。适用于绝对路径或已配置的 SDK profile |
| `KITE_DSH_PROVIDER` | DSH 已注册的模型提供方路由，缺省 `deepseek-official`；不是界面品牌名 |
| `KITE_DSH_MODEL` | 该路由真实支持的模型 ID，缺省 `deepseek-v4-flash`；本阶段会话内固定 |
| `KITE_DSH_HEALTH_URL` | 可选，实际模型服务的只读 models 查询地址；自定义路由应显式设置为与 DSH 配置相同的服务。HTTPS，隔离测试允许本机 HTTP |
| `DEEPSEEK_API_KEY` / `MIMO_API_KEY` | 分别用于 DeepSeek 或 `mimo-*` 模型的健康鉴权；DSH 仍使用自身已配置的同一模型路由与凭据，不由 Kite 改写其全局配置 |
| `DSH_HOME` | 可选，沿用 DSH 自己的 profile/存储配置 |

例如已配置 MiMo token-plan 路由的主机，应填写它的真实 provider ID、当前模型 ID、对应的 `KITE_DSH_HEALTH_URL` 和 `MIMO_API_KEY`；不要用另一个服务的健康结果代替当前路由。密钥不得写进 command argv、仓库、发布包、日志或客户端状态。

## 连接与展示

后端先验证配置，再进行带鉴权的只读模型查询及 SDK `initialize`。只返回稳定 provider ID 和操作能力，探测缓存最多 20 秒。客户端显示名称 DSH，初始化阶段锁住发送；该会话的聊天连接就绪后才可发送。模型、附件、Skill、额度、暂停和终端入口按当前不支持处理。

主机诊断可运行 `kite backend harness-scan`，查看 `providers` 中是否存在 `id=dsh`。它不开始模型 turn，也不返回密钥或消息正文。未出现时按开关 → 命令 → 凭据/模型路由 → 健康 URL → SDK profile 的顺序检查。查询成功不代替真实模型验证。

## 会话语义与限制

Kite 在 `~/.local/state/kite/harnesses/` 私有目录保存逻辑会话、owner 状态和规范化公开历史。本阶段支持 Kite 新建的 DSH SDK 会话，不接管外部 DSH TUI；手机断开后 owner 继续运行，重新连接复用它，逻辑归档不终止任务。

SDK 原生会话 ID 与 Kite 逻辑 ID 不同。owner 正常结束后，可创建新 SDK 会话，以保存的公开对话补齐上下文再接受新输入；不重放旧 prompt 或工具。发生运行中异常退出时，先检查历史再决定后续操作。恢复上下文有明确大小限制，超限报错并要求新会话，不静默截断历史。

SDK 暂未暴露的审批/问题响应能力不模拟成手机授权。DSH 自己的工具策略保持原样；需要人工交互的配置应使用其原生入口，不能通过 Kite 的普通聊天代替批准。

## 验证入口

- `python3 -m unittest discover -s backend -p 'test_kite_harness.py'`：隔离 SDK 子进程、模型健康夹具、隐藏条件、连续对话与唯一 owner。
- Rust 的 `kite-agent` / `kite-app` / `kite-ffi` 检查：目录、消息、历史、会话、绑定与已有 provider 回归。
- Android 的 `SessionInitializationTest` 和原生展示检查：DSH 名称、目录选项、发送就绪及按能力显示。
- 真实模型使用独立的自有会话验证首条回复、工具结果与重连，单独记录结果，不以夹具或进程存在替代。
