# 双屏远程键盘在视频 Surface 重建后失效

Alpha 92 的工作区在 `VideoSurface` 的 renderer 销毁回调中，同时清除了工具栏使用的 `videoReference` 和文字 flush 回调。Android 可以只重建同一控件内的 Surface；该控件没有退出 Compose，`AndroidView.factory` 不会重新执行。新 renderer 已就绪，工具栏引用却一直为空，点击“键盘”便无法打开输入法。

本次将 renderer 的销毁继续交给 `vm.videoGone`，把控件引用的清理移到 `AndroidView.onRelease`。清理时按控件身份匹配，避免旧控件清掉新控件的绑定；释放键盘时只隐藏该控件仍在持有的远程编辑器。沿用 Alpha 84 的视图/窗口焦点等待和取消逻辑。[AndroidView 官方生命周期说明](https://developer.android.com/reference/kotlin/androidx/compose/ui/viewinterop/AndroidView.composable)。

## 原生对照

使用 Alpha 92 同源的既有 x86_64 Debug APK，加上新增的 `SplitKeyboardLifecycleTest`，在隔离、禁网的 Android API 36 模拟器运行真实双屏工作区。基线两个用例中一个失败：首次打开正常，同一控件的 Surface 第一次销毁并重建后，“键盘”按钮不能再次唤起输入法。聊天/触控板切换用例通过。[基线失败截图](baseline-failure.png)。

修复后 **6 项原生测试全部通过**：

- `toolbarReopensAfterTheSameVideoViewRecreatesItsSurface`：连续三轮实际 `SurfaceHolder` 销毁与重建，要求保留同一个 `VideoSurface` 对象，renderer ID 先归零再改变；每轮通过工作区键盘按钮打开真实系统 IME，并确认 IME 正绑定该视频控件，文字 flush 回调也仍可达。[修复后双屏截图](after-recreation.png)。
- `splitToolbarSwitchesBetweenChatTouchpadAndRemoteInput`：三轮聊天输入框 → 远程键盘 → 触控板 → 远程键盘切换，检查 IME 的显示与绑定。
- 既有 `RemoteKeyboardTest` 三项：回答框/远程编辑器和返回键的多轮切换、真实 Dialog 的窗口焦点恢复及显式取消、焦点获取失败后重试。
- 既有输入法组合文本及 Enter 提交顺序回归一项。

**85 项 Release JVM 测试**、Release lint、Release Kotlin 和 instrumentation 编译通过。Rust/UniFFI 从同工作区构建；Rust 源码与 FFI 接口未变。原生验证数据见 [native-validation.json](native-validation.json)。

测试使用空白隔离应用及合成桌面状态，不连接主机、不发送模型请求、不验证远端字符接收。复用了自有 QA 模拟器数据；本次启动的 Android System UI 未响应弹窗在运行测试前已关闭。实体手机和 OEM 输入法尚未实测。用户主机、会话和配对配置未修改。

发布版本为 **0.2.0-alpha.93 / versionCode 95**，基于 Alpha 92，仅需更新 Android 客户端。

[Alpha 93 已发布](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.93)，源码为 `0261bf24e566e1a64872dd8781efa500535749f1`。[标签构建与发布](https://github.com/wenshiqi0/Kite/actions/runs/42886781304) 的两项任务均通过：可信构建验证 SDK 签名、既有证书、API 23/36、APK payload 一致及 16 KiB 对齐，发布任务重新下载全部五个附件核对字节。另独立读取公开 manifest 与 SHA256SUMS，确认源码/版本/Release/ABI、Alpha 92 证书 pin，以及全部四项 checksum 与 GitHub 服务端附件 digest 一致。[发布核验记录](published-release.json)。
