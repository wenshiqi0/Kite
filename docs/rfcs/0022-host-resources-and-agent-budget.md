# RFC 0022：主机资源概览与共享 Agent 内存预算

> 编号：0022 · 状态：已实现 · 创建：2026-11-02
> 相关：RFC 0014、0017、0018、0020、0021

## 背景与目标

客户端需要在主机详情查看 CPU、内存、压力和高占用进程。用户确认：内存严重不足时预警并限制新建/恢复；本机所有 Kite agent 及本地工具、构建子进程合计使用 10 GiB，可在客户端修改。控制服务保持在预算之外。

## 数据与展示

Linux 适配器只提供 `/proc/stat`、`MemAvailable`、Swap、PSI、cgroup 计数和前八个常驻内存进程的事实。CPU 通过两次计数差计算；首次样本保持未知，不显示虚构的零。进程列表不含完整命令、环境或消息。

Rust Node 每两秒采样，Core 使用独立任务同步，不等待聊天历史或 provider 探测。主机、连接代次和断线状态限制发布范围。FFI 与原生库同版生成；Android 展示主机详情、预算编辑、聊天预警及后台系统通知。系统通知受权限控制，按主机去重并限频，锁屏隐藏详细资源信息，点击仅打开主机详情。

## 限额与启动

`node.json.agent_memory_limit_bytes` 保存预算，当前主机为 `10737418240`。Linux 使用用户级 `kite-agents.slice` 和逐 owner 的 transient scope；Codex、Claude pane 与 DSH owner 在执行前进入预算，工具子进程继承。独立容器和外部系统服务单独计入主机总内存，不伪装成受此进程预算管理。

Linux 不迁移已分配内存的 cgroup 账目。现有进程保持运行，按其迁移前 RSS、Swap 和小额内核开销保守预留；内核新分配上限相应减去预留，原进程退出后释放。客户端显示该预留。Scope 生命周期核对 PID/start ticks，恢复与清理共享 session lease，避免误停冷恢复中的终端。

实际用量统一用于面板、预警、启动准入与调整上限：从 cgroup 总量扣除可回收的未映射文件缓存及 `slab_reclaimable`，再加迁移预留。文件缓存按 `max(0, file - shmem - file_mapped - file_dirty - file_writeback)` 保守计算，不把 tmpfs、常驻文件映射或尚待写回的数据当成可立即回收的缓存；active/inactive 是内核回收队列，active 文件缓存也不等于进程常驻内存。缓存独立展示，不再从实际用量重复扣减，也不能抵扣迁移预留。

预警使用主机可用内存、持续 PSI 和实际 agent 用量。预算达到 80% 预警、90% 严重；主机可用量低于 5% 或 512 MiB 时严重。恢复需三个健康样本。新启动按最多 1 GiB 预留，串行原子登记并持久化，120 秒超时或初始化完成释放，避免并发启动超额。

严重压力、未落实限额或预留不足时拒绝新启动；已有 owner 复用和历史读取继续。调低预算不得低于当前实际用量与启动预留。Linux 的 `memory.max` 计入缓存，因此保留不高于预算减迁移预留的内核硬上限，内核随常驻内存增长回收缓存；不能把缓存量加回硬上限，否则缓存被回收后可能允许实际内存超额。安全调低时先用 `memory.reclaim` 回收缓存并确认总量留出余量，再设置硬上限；回收失败保留原限额。重申或提高现有限额不因当前缓存接近硬上限而误报保护失效。到达硬上限且无法回收时可能终止超额进程，客户端保留历史并显示 OOM 事实，不自动重放任务。

不将提前预警实现为低于预算的强制 `memory.high` 节流。本机验证发现这会使构建反复回收并长时间等待。软边界与硬边界相同，预警和启动准入在 Rust 中提前执行。

## 验证与依据

覆盖 CPU 差分、PID 复用、缓存、压力恢复、并发预留、设置持久化、损坏账本、真实 64 MiB 隔离 OOM，以及 Android 展示、编辑与系统通知。真实限额验证记录与发行包分别提供证据。

- [systemd resource control](https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html)
- [cgroup v2 memory ownership](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-ownership)
- [Linux PSI](https://docs.kernel.org/accounting/psi.html)
