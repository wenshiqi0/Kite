# RFC 0008：原生富文本回复与稳定视频布局

> 状态：已实现（Android Alpha 8） · 2026-10-30

模型回复保存的 Markdown 原文没有问题；Android 将它交给普通 `Text` 导致标记直接显示。采用 Markwon 4.6.2（core、tables、strikethrough、tasklist）在原生层解析并绘制标题、强调、列表、引用、代码、链接和表格。用户输入、工具参数与工具输出保留原来的文字展示；公开的 assistant 进度使用同一富文本组件。流式快照只在文字改变时重新解析，历史数据与后端协议保持原样。

不安装 HTML 或网络图片插件；普通网页、邮件链接由系统打开，其余 URI 不触发应用或本地文件操作。附件继续使用现有附件组件。原生文字支持选择；表格使用 Markwon 的原生表格 span，窄屏按可用宽度换行，完整回复另提供复制入口，避免表格 span 的系统选择限制丢失内容。

视频继续使用同一个 SurfaceView 和源桌面坐标系。父视口在 measure 阶段确定视频平面的实际尺寸，layout 同步定位并按实际尺寸计算缩放；不再在 onSizeChanged 内修改子视图 LayoutParams 并依赖下一次外部布局。Surface 获得有效尺寸后才创建渲染器；分屏尺寸改变不重建已有渲染器。保留 1080p 源分辨率、等比适配、观看与操作下的缩放和输入映射，以及 PiP 的全画面适配。

本次仅调整原生展示，不迁移记录、身份、信任或配对，不改变 Rust/FFI/后端接口。验收包括原生富文本 span、增量文本重新测量、首次分屏的子 Surface 尺寸、布局切换与缩放后的坐标一致性，以及设备截图与真实视频首帧。

依据：[Markwon 4.6.2](https://github.com/noties/Markwon/releases/tag/v4.6.2)、[表格插件及选择限制](https://noties.io/Markwon/docs/v4/ext-tables/)、[Android measure/layout](https://developer.android.com/reference/android/view/View)、[Surface 生命周期](https://developer.android.com/reference/android/view/SurfaceHolder.Callback)。
