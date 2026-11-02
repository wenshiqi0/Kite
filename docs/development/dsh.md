# DSH 主机配置与验证

> 内部开发与排查资料；按需从 [Harness 接入规范](harness-integration.md) 进入。客户端能力合同见 [RFC 0022](../rfcs/0022-harness-capability-parity.md)，Linux Ply 另见 [Ply 配置](ply.md)。

## 启用

DSH 使用原生 ACP profile。开关、命令、模型服务鉴权、目标模型与原生协议均验证成功后才进入客户端下拉框；关闭或失败时不展示，已有历史保留。探测只读，不创建模型 turn。

主机私有 `~/.config/kite/harness.env`（0600）或 `node.json.environment`：

```dotenv
KITE_ENABLE_DSH=1
KITE_DSH_PROVIDER=deepseek-official
KITE_DSH_MODEL=deepseek-flash
DEEPSEEK_API_KEY=主机真实密钥
```

| 变量 | 行为 |
| --- | --- |
| `KITE_ENABLE_DSH` | 仅精确值 `1` 开启 |
| `KITE_DSH_COMMAND` | 可选 JSON argv，默认 `["dsh","--profile","acp"]`；不经 shell，必须支持 ACP 和 `--patch` |
| `KITE_DSH_PROVIDER` | 原生已注册路由，默认 `deepseek-official` |
| `KITE_DSH_MODEL` | 初始模型；之后可在客户端选择该路由下的可用模型 |
| `KITE_DSH_HEALTH_URL` | 自定义路由必填，指向同一后端的 models 地址；HTTPS，测试允许本机 HTTP |
| `DEEPSEEK_API_KEY` / `MIMO_API_KEY` | 当前后端健康鉴权；DSH 原生路由必须使用同一凭据 |
| `DSH_HOME` | 原生配置及持久存储；不改写全局用户设置 |

`KITE_DSH_MODEL` 未设置时仍使用 `deepseek-v4-flash`。不要按名称推断视觉能力：本机 DSH 原生目录的 `deepseek-flash` 提供 text/image 和 off/low/high/max，`deepseek-v4-flash` 是另一个模型 ID。客户端展示原生目录与已鉴权服务目录的交集，不把其他路由的凭据或健康结果混入。

## 模型、视觉与控制

Kite 从 DSH 自身 LLM 服务读取模型名称、输入模态、思考选项和默认值。一个只读元数据插件等待原生 loader 就绪后输出这些公共字段；不导出请求配置、密钥或隐藏思考。ACP 的 `session/set_config_option` 接收原生模型标识和精确 effort 值，没有统一硬编码的模型或思考列表。

图片上传后转成 ACP image 内容块。DSH 按自己的附件存储和 `read_image` 流程把像素送入模型；文件路径清单不代替图片输入。ACP 图片声明在 loader 就绪后协商；从文本模型切到视觉模型时，必要时在空闲边界重新协商并恢复同一原生 session。

运行中补充文本通过 DSH 自己的 `agent.steer` 进入下一步边界；ACP 单次 prompt RPC 的限制不再变成客户端“运行中不能追加”。主机桥接使用同进程内的原生 inbox，私有 Unix socket 不对外开放。边界已结束、切换模型或图片输入则按顺序进入下一轮，不重复发送。

ACP 工具调用与结果按 ID 关联；权限请求保留原生 request ID，客户端仅能批准或拒绝当前会话的单次请求。断开客户端不结束 turn，重新连接可看到待审批。取消走原生 `session/cancel`，不使用进程暂停冒充取消。额度、显式 Skill 和终端未接入时不展示对应入口。

## 身份与迁移

Kite 在 `~/.local/state/kite/harnesses/` 保存逻辑会话、唯一 owner、原生引用和公开历史，文件保持私有。ACP 原生 session ID 单独持久化；重启恢复原生会话，不重放用户 prompt 或工具。

旧 SDK owner 仅能在空闲时迁移，活跃任务不会被替换；旧公开历史在首次 ACP turn 作为上下文带入一次。运行中断或身份不明时明确报错，不重复创建 owner。归档不终止任务。

## 验证

`python3 -m unittest discover -s backend -p 'test_kite_harness.py'` 覆盖鉴权与隐藏、模型/effort、原生图片块、工具、审批重连、取消和唯一 owner。真实模型、Android 展示及发布证据保存在本轮产品验收记录；夹具不代替真实调用。
