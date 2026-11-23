# 系统架构、仓库布局与依赖边界

> 所属规范：[SPEC.md](../../SPEC.md) · 状态：生效 · 更新：2026-11-23
> 相关提案：[RFC 0001](../../rfcs/0001-architecture.md)、[RFC 0014](../../rfcs/0014-linux-host-node.md)、[RFC 0024](../../rfcs/0024-bun-server-monorepo.md)

## 1. 仓库布局

```text
SPEC.md                  工程规范根目录入口与总索引
docs/
  spec/                  分模块生效工程规范
  development/           内部开发实施规范（harness 接入、Bun 服务端、跨供应商协作等）
  agent/                 `kite docs` 嵌入式运行时 Agent 指南（单一事实来源）
  product/               产品稿与分阶段验收记录
  release-notes/         各版本发布说明
  *.md                   部署、构建、测试、发布与平台使用指南
rfcs/                    独立设计提案目录（RFC 0000 模板、README 索引及全部编号提案）
Cargo.toml, Cargo.lock   Rust workspace 根，成员为 crates/*
crates/<crate>/          Rust 核心、主机节点、FFI 边界与 Linux GTK 面板
package.json, bun.lock   Bun workspace 根，成员为 packages/*
packages/<package>/      TypeScript 服务端协议、机制、provider 适配与 Linux 交付工具包
desktop/linux/           Linux 原生主机控制面板与用户级安装入口
deploy/docker/           Docker Linux 纯服务端镜像、systemd 配置与运行脚本
android/                 Android 原生应用（Kotlin / Compose，Gradle 工程）
ios/                     iOS 原生应用（SwiftUI，XcodeGen 工程 `ios/project.yml`）
macos/                   macOS 原生客户端与本机主机面板（SwiftUI/AppKit，`macos/project.yml`）
windows/                 Windows x64 原生纯客户端（C# / WPF）与 Linux 交叉构建脚本
skills/kite/             轻量标准 Agent Skill 发现包（仅保留薄索引，正文在 `docs/agent/`）
scripts/                 边界检查、主机 bundle 构建与发布校验脚本
target/                  构建输出（不提交）
```

### 目录与提交红线

- 构建生成物一律不得提交：`target/`、`android/app/build/`、`ios/build/`、`ios/Generated/`、`ios/Kite.xcodeproj`、`macos/build/`、`macos/Generated/`、`macos/Kite.xcodeproj` 及主机 bundle 产物。
- Rust 代码只放在 `crates/` 下，不得再使用旧 `core/` 目录。
- crate 名统一采用 `kite-` 前缀加小写短横线分隔；唯一例外是工具 crate `uniffi-bindgen`。
- Rust 第三方依赖版本统一声明在根 `Cargo.toml` 的 `[workspace.dependencies]`，各 crate 使用 `xxx.workspace = true` 引用。
- 客户端界面必须全部使用平台原生语言实现：Android Kotlin、iOS/macOS Swift、Linux Rust/GTK、Windows C#/WPF；不得将客户端 UI 迁入 Bun/TypeScript，也不得保留 Python GUI。

---

## 2. Rust Crate 分层与依赖边界

### 2.1 分层模型

```text
原生应用（Kotlin / Swift / C#）                 Linux 面板（kite-desktop，独立 CLI 调用）
        │  UniFFI / Windows C ABI · 独立媒体通道
边界层   kite-ffi          唯一对外客户端库（Android cdylib / Apple staticlib / Windows DLL）
        │
可执行   kite-node         Linux / macOS 主机节点 CLI（仅启用 host-node 特性）
        │
应用层   kite-app          多主机运行时、视图模型快照、命令编排与 host_node 运行时
        │
领域层   kite-herdr   kite-agent   kite-stream
        │
基础层   kite-ssh     kite-store
```

### 2.2 各 Crate 职责

| crate | 层级 | 职责 |
| --- | --- | --- |
| `kite-ssh` | 基础 | SSH 连接、ed25519 密钥生成与解析、严格主机密钥校验、命令执行、`direct-tcpip` 端口转发与保活 |
| `kite-store` | 基础 | 本地持久化：主机配置、已信任主机指纹、会话与对话缓存、排序/权限设置、本机订阅状态（数据目录由原生层注入） |
| `kite-herdr` | 领域 | 查找 herdr 并校验桥接、JSON API 客户端（agent 列表、事件、发消息、读屏、发按键）与 pane 模型 |
| `kite-agent` | 领域 | 对话历史定位与解析、统一 Item/Block 与 Thread 模型、结构化聊天与有界附件传输、模型/额度/Skill/通知与状态行 `ActivityLine` 推导 |
| `kite-stream` | 领域 | Sunshine 配对与会话管理、通过 `moonlight-common-c` 接收视频/音频帧并输出至 `MediaSink`、串流输入编码 |
| `kite-app` | 应用 | 编排领域 crate：多主机独立工作区、连接与重连生命周期、带 revision 的快照与增量推送、唯一 tokio 运行时；可选 `host-node` 特性承载主机节点与 MCP/ACP CLI |
| `kite-ffi` | 边界 | 唯一对外客户端库：UniFFI 导出、Windows 进程内 C ABI、Android JNI 与 Apple/Windows C 媒体通道、平台日志初始化；**不得包含业务逻辑** |
| `kite-node` | 可执行入口 | Linux/macOS 主机节点可执行入口，调用 `kite-app::host_node`，只负责参数与退出码 |
| `kite-desktop` | 原生桌面展示 | Linux GTK 主机控制面板与托盘展示层，仅调用 `kite-node` 结构化 CLI，不引入 provider 或服务策略 |
| `uniffi-bindgen` | 构建工具 | 从编译产物生成 Kotlin / Swift 绑定（每次构建生成，不提交仓库） |

### 2.3 允许的内部依赖

| crate | 允许依赖的内部 crate |
| --- | --- |
| `kite-ssh` | 无 |
| `kite-store` | 无 |
| `kite-herdr` | `kite-ssh` |
| `kite-agent` | `kite-ssh`、`kite-herdr` |
| `kite-stream` | `kite-ssh` |
| `kite-app` | `kite-ssh`、`kite-store`、`kite-herdr`、`kite-agent`、`kite-stream` |
| `kite-ffi` | 仅 `kite-app` |
| `kite-node` | 仅 `kite-app`（`features = ["host-node"]`） |
| `kite-desktop` | 无内部依赖 |

硬性规则：

- 依赖只能严格自上而下；表中未列出的内部依赖及同层互依赖一律禁止。
- `kite-ffi` 所需的领域类型必须由 `kite-app` 通过 `pub use` 重导出。
- 领域层与基础层不得依赖 `uniffi`、`jni`、`android_logger` 或任何平台 SDK 绑定。
- 只有 `kite-ffi` 可以使用 UniFFI 宏以及 `cfg(target_os = "android")` / `cfg(target_os = "ios")` 条件编译分支。

### 2.4 Rust 内部约定

- **错误**：每个 crate 用 `thiserror` 定义独立错误枚举，公开 API 不得返回 `anyhow::Error`；上层通过 `#[from]` 或显式映射转换。
- **异步运行时**：统一使用 `tokio`。只有 `kite-app` 可以创建运行时，且整个进程只有一个多线程运行时。其他 crate 只暴露 `async fn`；派生后台任务必须将 `JoinHandle` 或取消控制权交还调用方。
- **超时**：所有网络、SSH 和 IPC 操作必须带明确超时。
- **日志**：使用 `log` 宏；不得记录私钥、token、口令或对话内容（允许记录主机地址与指纹）；`info` 级别只记录状态变化。
- **文案**：Rust 中不得包含面向用户的 UI 文案；`Display` 仅用于英文诊断日志。
- **测试**：基础层与领域层测试不得依赖外网或真实远端主机；SSH 测试使用进程内测试服务端（参照 [ssh_check.rs](../../crates/kite-herdr/tests/ssh_check.rs)）。

---

## 3. Bun 服务端 Packages 架构与边界

按 [RFC 0024](../../rfcs/0024-bun-server-monorepo.md)，仓库并行维护 Cargo workspace（`crates/*`）与 Bun workspace（`packages/*`）。服务端平台机制、provider 适配器、网络 ACP 网关与 Linux 安装交付工具统一使用 TypeScript（strict 模式）+ Bun 运行时，彻底取代旧 `backend/` Python 服务端路径。

### 3.1 包分层与单向依赖

依赖边界由 [scripts/check-boundaries.ts](../../scripts/check-boundaries.ts) 在 `bun run lint` 阶段自动强制校验：

| package | 职责 | 允许依赖的 `@kite/*` 包 |
| --- | --- | --- |
| `@kite/protocol` | 共享 JSON 校验原语、会话目录 schema、Provider 枚举（`PROVIDERS` / `CHAT_PROVIDERS` / `MANAGED_PROVIDERS`）、ACP 帧校验与上限常量 | 无 |
| `@kite/runtime` | 文件原子写入、锁、进程/socket/RPC 原语、网络 ACP WebSocket 服务端与客户端传输机制 | `@kite/protocol` |
| `@kite/platform` | 宿主系统机制：Linux cgroup v2 资源采集与预算执行、X11 桌面观测/输入/启动适配、系统服务探测 | `@kite/protocol`、`@kite/runtime` |
| `@kite/providers` | 会话目录与恢复锁、统一 owner 生命周期（`HarnessLifecycle`）、Codex/Claude/DSH/Ply/Antigravity 适配器、附件与历史投影、网络 ACP 网关逻辑（`KiteAcp`） | `@kite/protocol`、`@kite/runtime`、`@kite/platform` |
| `@kite/host` | 主机适配器统一 CLI 入口（含 `acp`、`proxy`、会话/历史/桌面/资源子命令） | `@kite/protocol`、`@kite/runtime`、`@kite/platform`、`@kite/providers` |
| `@kite/linux-host` | Linux 用户级安装器、打包器与 Docker 部署脚本 | `@kite/protocol`、`@kite/runtime` |

### 3.2 TypeScript 与构建约定

- **严格校验**：所有外部输入 JSON 必须以 `unknown` 入口并通过 `@kite/protocol` 校验函数收窄；禁止 `any`、禁止放宽 `tsconfig.json` strict 检查、禁止忽略类型错误。
- **测试隔离**：每个包下 `src/` 仅放生产代码，`test/` 放单测、夹具与验收脚本；生产源码不得导入 `test/`、`fixtures` 或 `bun:test`，也不得跨包使用相对路径逃逸。
- **无 Python 服务端依赖**：`packages/*/src/` 及 `desktop/linux/` 不得包含 `.py` 或 `python3` 运行时依赖（发布与部分历史 QA 工具脚本位于 `android/scripts/`，不属于服务端运行时）。
- **构建产物嵌入**：`scripts/build-host.ts` 与 `crates/kite-agent/build.rs` 将服务端脚本打包进入 Cargo `OUT_DIR`，运行时按源码摘要释放到私有目录；构建产物不得提交仓库。
