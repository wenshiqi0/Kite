# Kite 项目协作约定

- 用户询问问题时，提供可核查的具体证据来源。
- 本项目桌面 UI 测试默认使用 Codex 自带 Computer Use（`cua_repl`）观察和操作；仅在用户明确要求时使用 Tactero。使用运行中的安装版完整路径选择 Kite，避免同 bundle ID 的构建/验证副本造成混淆。测试结论区分隔离夹具与当前真实主机连接。
- 仅在新增、修改或排查聊天 harness/provider 接入时，按需读取 [内部接入规范](docs/development/harness-integration.md)；普通功能开发和 Kite CLI/MCP 使用不加载该文档。
- 重构前确认兼容面，不自动增加旧接口兼容代码。本次已经确认：迁移 M1 主机与全部 SSH 信任记录，复用设备 SSH 私钥；FFI 库与绑定同次生成，不保留旧 FFI 入口。
- 本地测试仍由各开发环节处理，可以生成本地测试包；正式交付包必须由 GitHub 标签 CI 构建、验证并发布到 GitHub Release，仅交付本地安装包链接不算完成。
- 正式版使用 `X.Y.Z` / `vX.Y.Z`，发布为非 prerelease 并标为 latest；Alpha 使用 `X.Y.Z-alpha.N` / `vX.Y.Z-alpha.N`，必须标记为 prerelease，不能标为 latest。每次新发布递增版本号和 Android versionCode。
- GitHub 的 `Publish Release` 工作流是正式发版入口：从 `main` 输入新版本和发布说明，自动更新 `android/version.properties`、提交版本、创建注释标签并启动同标签构建。`android/scripts/release-alpha.py` 保留既有 CLI 和构建/发布校验。源码必须已提交；标签和附件不可覆盖，重复执行只能恢复同一提交、同一内容的发布。
- 发布签名密钥必须稳定并留在仓库外；CI 签名材料只能配置到 GitHub Actions encrypted secrets，不得提交到源码、附件或日志。设备私钥与本机测试授权不上传。发布前后保留其他工作区的未提交修改。
- Android CI 在 main / PR 构建 Release 变体；PR 只生成未签名校验包，不获得签名密钥。标签 CI 同时验证 Android/Linux 包和 Windows 客户端，通过后将全部产物发布到同一 GitHub Release；对用户交付包以该 Release 附件为准。

工程架构遵循 [SPEC.md](SPEC.md)，发布方法见 [docs/releases.md](docs/releases.md)。
