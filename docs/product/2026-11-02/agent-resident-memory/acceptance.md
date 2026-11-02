# Agent 实际内存预算

原面板把 cgroup 的 `memory.current` 全部显示为 Agent 用量，而启动判断仅扣除 `inactive_file`。文件读写留下的缓存因此可能让 10 GiB 进度条接近满，即使主机还有大量可用内存。调低限额也错误地要求含缓存的当前总量必须低于新上限。

## 实现

实际用量为 cgroup 总量减可回收缓存，再加已有进程迁移预留；启动预留额外计入准入。缓存按 `max(0, file - shmem - file_mapped - file_dirty - file_writeback) + slab_reclaimable` 保守计算，并限制不超过总量。tmpfs、文件常驻映射和未写回数据不扣除。缓存不能抵扣迁移预留，也不能在准入时重复扣除。

Linux 内核总量硬上限保持为预算减迁移预留，不因缓存抬高。缓存随真实分配增长由内核回收。调低上限前先通过 `memory.reclaim` 回收足够缓存、核对含缓存总量留出 16 MiB 余量，再写入上限；无法确认余量时拒绝修改，避免调低触发 OOM。重申或提高原有上限无需额外空闲余量。

Android 将缓存说明改为“可回收缓存（未计入用量）”。现有资源协议和 FFI 字段保持相同，旧客户端收到的 Agent 用量也会按实际占用更新。

## 本地验证

- `bun run check`：类型、lint、依赖边界和 36 个测试通过。
- `cargo test -p kite-app --features host-node --locked`：71 个测试通过，含缓存扣除一次、迁移预留、实际高占用预警、允许缓存支撑的安全调低、拒绝真实占用超额等回归。
- `cargo clippy -p kite-app --features host-node --all-targets --locked -- -D warnings` 和 `cargo fmt --all -- --check` 通过。
- `bun run scripts/resource-budget-probe.ts`：真实 systemd 隔离组和独立 Python 进程通过。使用磁盘目录而非 tmpfs；文件缓存约 165 MiB，实际用量低于 1 MiB；回收缓存后将内核上限从 256 MiB 调低至 64 MiB；32 MiB 常驻分配计入实际用量并继续运行；随后 256 MiB 超额分配被该隔离组的内核 OOM 终止。测试只清理自有进程、文件和唯一命名的资源组。

标签 CI、公开包校验和本机升级结果在发布后另行记录。本地检查不代表已经更新手机上的 APK，也未重复物理手机交互或媒体测试。
