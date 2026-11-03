# RFC 0028：Docker Linux 纯服务端部署

> 编号：0028 · 状态：已实现 · 创建：2026-11-03
> 相关：RFC 0014、0018、0019、0022、0024

## 背景与目标

在现有用户级 Linux 安装之外，增加可部署到服务器的纯服务端镜像。手机继续经 SSH 使用同一 Rust Node、Herdr API、Sunshine/Moonlight 媒体通道及 Bun 适配器。容器自行提供 X11/Xfce 桌面、虚拟声音、中文字体与输入法。

## 设计

- `deploy/docker` 维护 Ubuntu 24.04 x86_64 多阶段镜像、Compose、systemd 单元及启动配置；`@kite/linux-host` 提供服务器部署命令。构建只接收显式允许的源码，镜像只编译并安装 `kite-node`，通过独立 Bun 初始化器准备服务端配置与命令别名；不打包 `kite-desktop`、图形管理面板、托盘或任何 Kite 客户端。Bun、Herdr、Sunshine 固定版本并校验下载摘要。默认只包含 Kite 服务和桌面必需组件，使用英文 C.UTF-8，包含中文字体及拼音输入法；不预装浏览器、微信、VS Code 或飞书。Codex CLI 可通过构建参数选择，使用固定 npm 版本与官方 registry。
- systemd 是 PID 1，使用私有、可写的 cgroup v2；用户 1000 的 manager 获得控制器委派。Node 仍使用既有 socket activation 和用户服务；agent 内存限制保持真实内核执行。容器替换属于维护重启，不声明保留进程或媒体连接。
- 启动前核验 Xfce 安装和中文字体，健康检查核验 Xfce 及输入法进程。Xorg dummy 提供被客户端连接的服务端显示，Xfce 与 Herdr、Sunshine、Node 都属于容器的 `kite` 用户。PulseAudio 提供 `kite_audio` 虚拟 sink。默认软件编码，可按服务器设备选择 VAAPI/NVENC。
- Sunshine 的虚拟输入通过 Linux input/udev 热插拔进入 Xorg；忽略物理输入设备。使用 rootful Linux Docker 的 privileged 容器、host 网络、私有 PID/cgroup namespace，以及只读 `/dev/input`、`/run/udev`。不挂载宿主桌面、用户 home、SSH 私钥、Docker socket 或宿主 cgroup 根。运行模型与 GUI 的用户保持非 root。这是信任宿主的完整桌面部署，不能用作运行不可信镜像的隔离边界；每台服务器仅支持一个此类 Sunshine 桌面实例。
- 默认 SSH 2222、Sunshine 基准端口 48089，避免默认宿主服务端口。媒体 TCP/UDP 仍直接连接同一服务器端口，管理 UI 仅允许本机，通过 SSH 转发配置。rootless Docker、Docker Desktop 和非 Linux 服务器不在此部署合同内。
- home 卷保存项目、用户偏好、Kite 身份/目录/上传、provider 登录和 Sunshine 配对；identity 卷保存 SSH host key 与 machine-id。每次启动准备镜像版本所需的服务端配置，保留用户配置。公开授权 key 单独传输并合并，镜像不含凭据。
- CLI 经指定 Docker context 操作 daemon，先检查 Linux/cgroup v2/rootful，再创建指定 Compose project，经 `docker cp` 传入公开 key 并等待健康；支持 SSH context，避免将本地路径误当成远端 bind mount。失败保留容器与数据以便诊断。

## 备选方案

仅包装 Node 不能提供真实桌面、声音和输入；自写替代 systemd 的 supervisor 会分叉现有服务、升级和资源预算机制。宿主桌面共享会混合已有用户会话与新部署的生命周期。因此新增具备被采集桌面的纯服务端镜像，并继续支持原来的用户级安装。

## 验证

部署命令测试覆盖远端传输、健康等待、参数拒绝和失败保留数据；镜像验收创建自有 project，检查真实 SSH、Herdr、X11 截图、声音与 cgroup，以受限 localhost 端口和专有 GTK 目标验证真实视频/Opus 包、键盘和鼠标；随后替换容器验证身份与文件保留。CI 在独立 runner 上执行相同验收。实际手机视频、声音与接管的端到端验收单独记录，服务存活不等同于媒体验收。
