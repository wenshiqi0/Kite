# RFC 0016：Android 远程桌面声音

> 状态：已采纳，实施中 · 2026-11-01

用户要求在本轮新会话和连接修复中同时支持远程桌面声音。聊天附件上传维持现有行为。本阶段接收 Sunshine 系统声音，在 Android 原生播放，支持本机静音、音量、音频焦点及串流生命周期。

沿用 Moonlight 的 Opus 音频传输和加密。C 适配层将格式与有界 Opus 包交给 `kite-stream::MediaSink`，Android 的专用 JNI 媒体通道转交原生 MediaCodec 和 AudioTrack；音频包和 PCM 不走 UniFFI、聊天或持久化。视频与音频各自有工作线程和有界缓冲，音频设备/解码失败不得阻塞或中断画面。首阶段协商 48kHz 双声道，允许丢弃过期音频以追上实时画面。

静音偏好属于本机播放设置，默认开启声音并持久保存；系统媒体音量决定播放音量。只看与操作均可播放，PiP 保留声音；离开且不进入 PiP、断开串流或销毁目标时释放解码器、播放设备及音频焦点。失去音频焦点时不得继续有声播放，恢复后不回放旧积压。音频故障只显示结构化诊断，不保存声音内容。

服务端曾在切换/恢复默认音频设备后触发 PulseAudio 断言。本阶段请求保留主机本地播放，不再为了手机播放而切换主机默认音频设备；主机也会继续出声。这是避免设备切换的规避措施，不声明已经修复上游 PulseAudio。测试必须使用独立音频服务和测试信号，不改变用户默认音频路由。通过带声音的连接/断开测试后恢复生产 Sunshine 的音频开关，不能以长期禁用声音交付此功能。

验收分别记录有界 Opus 接收、实际 PCM 解码、AudioTrack 播放头前进、非静音测试信号、静音/恢复、断开资源释放、反复连接及视频帧率。模拟器播放管线与真实手机扬声器/Bluetooth 的听感、音画同步分开记录。保留已有历史、设备身份、信任和配对；C 适配层、Rust 和 JNI 同次构建，不保留旧媒体 ABI。

依据：[Android MediaCodec 的 Opus CSD](https://developer.android.com/reference/android/media/MediaCodec)、[AudioTrack](https://developer.android.com/reference/android/media/AudioTrack)、仓库固定版本 Moonlight 的 `OPUS_MULTISTREAM_CONFIGURATION`，以及安装版本 [Sunshine audio capture](https://github.com/LizardByte/Sunshine/blob/v2026.914.233613/src/audio.cpp)。
