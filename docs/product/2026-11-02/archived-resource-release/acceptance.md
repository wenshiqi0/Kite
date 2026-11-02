# 归档资源释放验收（2026-11-02）

资源释放属于托管 harness 的统一能力：`HarnessLifecycle` 提供只读状态、owner 租约、共享 owner 判定和结构化释放结果，目录声明 `release`。Codex、DSH、Ply 实现统一接口；没有受管 owner 的外部终端不声明释放能力。

- Android 已归档列表提供“释放后台进程”，经 Core/FFI 调用与自动归档相同的主机命令 `kite sessions release-archived`。
- 归档目录更新立即唤醒释放检查；任务执行中、等待审批、未知或恢复上下文未落盘时保留，结束后继续检查。
- 未归档会话没有闲置期限；旧 `idle_timeout` 请求也被拒绝。状态报告 `archivedOnly=true`。
- 释放前在恢复/输入的同一 owner 租约与目录锁内复核归档状态、原会话引用、PID/start ticks 和上下文指纹；不删除历史或重放消息。仅定向回收被释放 owner 的文件缓存，不主动交换活跃堆内存。
- 修正生命周期子进程未携带已安装适配器路径的问题，避免加载不存在的默认文件而使自动释放失效。

本地验证：Rust 工作区测试、host-node 生命周期命令测试、Clippy、71 项 Python 测试、73 项 Android JVM 测试、Release lint 与 Android 原生测试源码编译通过。覆盖 DSH/Ply 冷恢复后原生上下文与公开历史不变、输入与释放租约互斥、未归档不超时，以及新增适配器无需修改公共释放路由。Android 原生控件测试覆盖操作、进度和结果，当前主机缺少 KVM 权限，未执行设备测试，不声称实体手机验收。

主机使用同源 Alpha 51 安装包完成兼容交接，报告 ready 与 `archivedOnly=true`，身份和 Herdr/Sunshine PID 保留，原有两个未归档会话仍在。[主机记录](host-verification.json)。在正式主机新建 DSH/Ply 自有空会话，归档自动释放，再恢复到同一 provider 引用；原生上下文未变，未发起模型 turn，验收记录已移除。[实际 owner 验证](owned-owner-results.json)。

另修正只读/构建预检把 PR 合并提交误判为已发布标签冲突的问题。9 项发布器回归通过；正式发布和启动发布仍严格核对标签与源码，不允许移动标签或替换附件。

## 最终交付

[Alpha 51](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.51)，versionCode **53**，发布源提交 `8084dd4d107d903afb0f8c5338df4133883a425f`。[标签 CI](https://github.com/wenshiqi0/Kite/actions/runs/41381423123) 与 [PR 检查](https://github.com/wenshiqi0/Kite/actions/runs/41382146214) 均通过；源码经 [PR #7](https://github.com/wenshiqi0/Kite/pull/7) 合入 main。

公开 APK 与 Linux 包的校验和、稳定证书、v1/v2/v3 签名、主包/兼容包载荷一致性与 16KiB 对齐独立验证通过。[公开包记录](public-verification.json)。正式 CI Linux 包已再次通过兼容交接安装；运行中二进制与公开包校验和一致，ready、归档专用策略、身份和原服务 PID 保留，清理命令报告零失败。[正式主机记录](official-host-verification.json)。

[Android 兼容安装包](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.51/kite-0.2.0-alpha.51-arm64-v8a-compat-v2.apk)。不声称实体手机或 Android 模拟器执行验收。
