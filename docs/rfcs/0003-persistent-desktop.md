# RFC 0003：持久 herdr 会话与 Android 远程桌面

> 编号：0003 · 状态：已实现 · 创建：2026-10-29
> 相关：RFC 0001、RFC 0002

## 背景与目标

M1 仅检查 herdr，然后关闭 SSH。用户已要求继续完成“连接 herdr → 打开远程桌面 → 基础键鼠接管”，并明确要求迁移旧版主机和信任记录、复用设备 SSH 私钥。本轮实现这条实际链路；结构化对话记录、终端单元格渲染、音频、画中画和跨主机并发串流不在本轮范围内。

## 设计

- 新建规范中已有规划的 `kite-store`、`kite-stream`。依赖方向保持 SPEC 第 3 节的规定。`kite-store` 保存主机、SSH 主机公钥和公开配对信息；私钥仍由原生 `SecretStore` 保存。
- 应用进程只有一个 `KiteCore`，由 `kite-app` 持有唯一 Tokio 运行时。初始化、迁移、连接和串流均为异步命令。状态带递增 revision，通过 observer 推送；原生层只做显示、输入与生命周期管理。
- SSH 连接持续保留。herdr JSON API 通过同一连接的 exec 通道调用官方 `remote-api-bridge`，以 herdr 0.9.5 的实际 NDJSON 契约为依据。请求有总超时和长度上限；后台活动由应用持有取消手段和任务句柄。
- Sunshine 的发现、配对及控制 HTTP/TLS 通过 SSH direct-tcpip 通道；视频与实时输入使用私有网络直连 Moonlight 协议。保持严格 SSH 主机密钥校验，未知指纹必须确认，变化的指纹必须拒绝。
- `kite-stream` 使用固定提交的官方 moonlight-common-c，连同其指定的 ENet、nanors 子模块构建，不链接系统 ENet。OpenSSL 用于 GameStream 配对、客户端证书和原生传输加密。生成的客户端私钥通过 `SecretStore` 加密存储；配对后的服务端证书与 SSH 主机身份绑定。
- 配对显示 PIN，支持正常的人工确认；主机已有授权管理凭据时，可通过 SSH 完成自动确认。管理凭据不得进入普通持久化、日志或快照。
- 原生 C 适配层隐藏 Moonlight ABI，`kite-stream::MediaSink` 不依赖平台 SDK。视频回调复用缓冲区。Android 的 `kite-ffi` JNI 适配把编码帧复制到原生预分配的 direct ByteBuffer，由 MediaCodec 工作线程解码并输出 Surface；视频字节不走 UniFFI。
- 视频尺寸来自主机 Sunshine 正在捕获的桌面，手机横竖屏只改变等比适配布局。当前自动探测支持同一 SSH 用户运行的 Linux X11 Sunshine：从监听端口定位进程，读取其 DISPLAY / XAUTHORITY 与 capture / output_name，以 XRandR 当前输出匹配真实尺寸；不读取 SSH 登录默认显示器或支持模式列表，不静默退回固定 720p。其他捕获后端的自动探测尚未实现。
- 优先以原尺寸请求 H.264、60fps；解码能力不足时可逐步等比降低尺寸，最多降低 15% 边长，以优先维持交互帧率；仍不支持才尝试 30fps。码率随像素数和帧率变化，上限 40Mbps；MediaCodec 开启低延迟，始终按完整源画面比例适配视口。当前没有网络拥塞触发的动态分辨率切换，也不播放音频。进入接管模式才转发触控、鼠标、滚轮、键盘和 UTF-8 输入。退出或连接断开时释放按下的输入。Surface 与解码器生命周期由 Android 管理，Rust 负责协议会话的启动、取消和停止。
- Android 使用前台服务管理连接存活。iOS 同步接入新的 Core/SecretStore 契约，保持现有连接能力；iOS 视频呈现不作为本轮验收目标。

## 迁移与兼容面

- 保留现有 Android Keystore wrapping key、`device_ed25519.enc` 和 iOS Keychain 设备密钥，不重新生成设备身份。
- 原生层导出 M1 的主机表单和全部已信任主机公钥。Rust 在原子持久化成功后才写入迁移标记；重试幂等。无效或冲突的数据报错，不以新文件覆盖原数据，也不默默信任新指纹。
- 地址规范化处理主机大小写和数值端口；合并地址时若公钥不同，必须保留错误供用户处理。
- FFI 库与绑定同次生成，不保留旧版 FFI 兼容入口。herdr 0.9.5 协议兼容属于本轮要求。

## 验收

基础层、领域层以本机进程内 SSH/HTTP 服务和协议夹具测试；不依赖外网或真实主机。之后构建 APK 并在 Android 模拟器连接 wen.linux，验证旧数据迁移、持久会话、真实配对、持续收到视频且 MediaCodec 实际呈现、以及输入抵达受控测试窗口。只完成编译或 `/serverinfo` 不算串流验收。测试保留原桌面模式、其他会话和 Sunshine 服务，撤销仅属于测试的授权。

## 对 SPEC 的影响

开始落实已规划的 `kite-store`、`kite-stream`、KiteCore、SecretStore 与 JNI 媒体通道；不改变分层或通道所有权。记录本轮迁移决策、单串流限制和上述验收边界。实现并验证后更新里程碑状态。
