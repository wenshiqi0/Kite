# Android 远程桌面键盘焦点修复

## 原因与证据

用户截图中，远端网页输入框已有光标，但手机系统键盘未显示。截图本身不能证明手机当时的窗口焦点或输入法状态；以下原因由源码和隔离原生对照测试确认。

Alpha 82 的 `VideoSurface.openRemoteKeyboard()` 忽略 `requestFocus()` 的结果，在没有窗口焦点时也只投递一次 `showSoftInput()`，随后返回成功。Android 要求窗口、编辑器焦点及输入法连接就绪；否则显示请求可能被忽略。见 [Android 官方说明](https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/visibility#ShowReliably)。

同一套 Android 16 原生夹具运行 Alpha 82 的生产 Kotlin 源码时，三个用例中两个失败：

- `failedFocusDoesNotLeaveARemoteEditor`：不可获取焦点的视图仍返回打开成功，并保留远程编辑状态。
- `pendingOpenWaitsForWindowFocusAndCanBeCancelled`：弹窗暂时占据窗口焦点时发出打开请求，弹窗关闭后键盘未恢复。

另一个源码问题是 `KiteWorkspace` 在任意 IME 隐藏回调中关闭远程编辑器。聊天回答框与远程编辑器交接时的隐藏帧也会走该分支，有取消新打开请求的风险。

## 改动

- `VideoSurface` 保存待执行的显示请求，在视图与窗口获得焦点后重新绑定输入法并使用 AndroidX 键盘控制器显示；显式关闭会取消待执行请求。
- 获取视图焦点失败时清理编辑状态并返回失败；返回本地回答框时释放远程编辑器，同时保留新编辑器的键盘。
- `KiteWorkspace` 读取实际远程编辑器状态，移除第二份打开标记，不再把暂时隐藏当作关闭请求。返回键、触控板、失去操作权及桌面销毁仍按各自的生命周期处理。

原生回归源码：[RemoteKeyboardTest.kt](../../../../android/app/src/androidTest/java/io/github/wenshiqi0/kite/RemoteKeyboardTest.kt)。发行基于 Alpha 83，保留已发布功能；本次未升级或重启用户的主机服务。

## 验证范围

原生测试使用独立 Android 16 / API 36 x86_64 KVM 模拟器、真实系统 IME、生产 `VideoSurface` 和 Compose 回答框。断言系统键盘可见且输入法服务已绑定远程编辑器，包含四轮回答框/远程输入切换及返回后重新打开。焦点恢复测试使用真实独立 Dialog 窗口，并验证已取消的请求不会重新打开键盘。

夹具未连接用户主机或发起模型请求，不声称已在实体手机上验收，也不把原生输入法测试表述为远端文本接收验收。x86_64 QA 的 Rust 库从同工作区源码构建并重新生成 UniFFI 绑定，沿用既有 NDK compiler-rt 显式链接方法；公开 ARM64 APK 由标准标签 CI 构建、签名和发布。

合并 Alpha 83 后的最终验证：84 项 Release JVM 测试通过；Release Kotlin、Debug instrumentation 编译及 lint 通过，lint 无 Error/Fatal。4 项 Android 原生测试全部通过，包括已有的输入法组合文本和 Enter 提交顺序回归。[系统键盘截图](remote-keyboard.png) 来自隔离夹具，视频区域没有连接远程主机。
