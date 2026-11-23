# RFC 0019：统一 Agent 文档与 Linux 桌面操作

> 状态：已实现（源码与本机验收，待发布） · 2026-11-01

用户要求把 Linux desktop 操作作为 Kite 的产品能力，通过统一 `kite docs` 渐进披露；首次发现和后续调用都控制上下文消耗，面向 Codex、Claude、DeepSeek Harness 等消费者，不能绑定某个 harness 的私有工具。

## 接口与职责

- 安装 `kite` 命令别名。`kite docs` 离线返回短索引；直接查询 `kite docs desktop` 即得到常用完整工作流，不要求先走多级目录。高级输入、主机、会话、集成与升级说明按主题读取，可一次获取多个相关主题。
- Markdown 是唯一文档正文源，编译进二进制，CLI 和 MCP 共用注册表及内容版本。调用方明确提供已保留在上下文中的主题 revision 时，未改变的正文不重复返回；不使用服务器全局“已经读过”标志。上下文压缩后缺失正文时不传 known。修订号由内容决定，不因无关发布变化而失效。
- `skills/kite/SKILL.md` 仅含标准元数据、入口及必要边界，保持轻量；不使用私有工具前缀、动态命令插值、厂商专用 frontmatter。安装器提供通用 `--skill-dir` 目标目录，不改 harness 的全局权限或配置。
- `kite desktop` 经现有私有 Node IPC 操作配置的 X11 桌面：状态、截图/区域截图、移动、点击、滚动、按键组合、stdin 文本、argv 应用启动。Rust 应用层验证动作/参数和目标，Python 平台适配器实现 X11/桌面会话机制；原生界面和移动端媒体通道不改变。
- 截图以私有 PNG 制品交付；MCP 返回标准 image block，禁止把 base64 当文本注入上下文。显式 known SHA 可省略完全相同的图像，区域截图返回全局坐标原点。图像保留/清理只作用于 Kite 自己的私有制品。
- MCP 保留既有只读主机/会话工具，新增短 `kite_docs` 和 `kite_desktop` 入口，具体操作说明按需读取。文档可在节点离线时使用；功能实际可用性由运行时查询报告，不能把说明书当作能力成功证据。

CLI/MCP 是 harness 无关合同。Agent Skills 是可选发现入口；任意能执行命令的 harness 都能使用 CLI。DeepSeek Harness 在本需求中是消费者例子，本阶段不新增其聊天/provider 管理。图像显示依赖客户端/模型能力，不能把返回文件路径当作已经看过图像。

## 边界与验证

外部访问继续沿用 SSH 和私有 0600 socket，不新增网络端口。桌面输入/启动动作会影响用户桌面，须属于当前任务授权；不自动通过 shell 解释命令，不记录输入正文/截图内容，不在不确定响应后自动重放修改。仅支持配置的 Linux X11 目标，不把 Xwayland、Wayland 或 macOS 声称为已交付。

验证包括：离线文档、内容失效/多主题增量、入口与完整阅读的 token 对照、MCP 标准图像、参数拒绝、隔离 X11 实际输入/区域坐标/文本/启动、标准 SKILL 格式及消费者契约。未做实际 harness 模型对话的范围明确记录。参考 [Agent Skills](https://agentskills.io/specification) 与 [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)。
