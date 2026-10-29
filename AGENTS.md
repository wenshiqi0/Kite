# Kite 项目协作约定

- 用户询问问题时，提供可核查的具体证据来源。
- 重构前确认兼容面，不自动增加旧接口兼容代码。本次已经确认：迁移 M1 主机与全部 SSH 信任记录，复用设备 SSH 私钥；FFI 库与绑定同次生成，不保留旧 FFI 入口。
- 每次 APK 打包完成后，必须将对应源码提交并推送到 GitHub，并发布包含 APK 的 GitHub Release；仅交付本地 APK 链接不算完成。
- 当前只发布 Alpha：版本使用 `X.Y.Z-alpha.N`，标签使用 `vX.Y.Z-alpha.N`，GitHub Release 必须标记为 prerelease，不能标为正式版或 latest。每次新发布递增 Alpha 序号和 Android versionCode。
- 使用 `android/version.properties` 维护 Android 版本，并通过 `python3 android/scripts/release-alpha.py --notes-file <发布说明文件>` 完成打包、验证、推送和发布。源码必须已提交；标签和附件不可覆盖，重复执行只能恢复同一提交、同一内容的发布。
- 发布签名密钥必须稳定并留在仓库外；不得将 keystore、口令、设备私钥或本机测试授权上传到 GitHub。发布前后保留其他工作区的未提交修改。

工程架构遵循 [SPEC.md](SPEC.md)，发布方法见 [docs/releases.md](docs/releases.md)。
