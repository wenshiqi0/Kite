# Docker host deployment

Docker is an additional **server-only** Linux deployment method. The image runs Kite Node, Herdr, Sunshine and Bun 1.3.14. Xfce/X11, a terminal/file manager, Chinese fonts/Pinyin input and virtual audio provide the server desktop captured by Sunshine. Phone and desktop clients connect from outside the container. The image does not build or install `kite-desktop`, a Kite control panel/tray, or a Kite client. The default image contains only Kite services and their required desktop components. Its language is English (`C.UTF-8`); it includes no browser, VS Code, WeChat or Feishu. Xfce executables and Chinese fonts are checked before services start, and desktop/IME processes are checked by container health. The phone uses the existing SSH and Sunshine connection flow.

## Server requirements

Use a dedicated Linux x86_64 server with rootful Docker Engine, Compose v2, systemd and cgroup v2. Allow at least 4 GiB RAM and 12 GiB disk for the desktop, plus the agents' workload. Load `uinput` and `uhid` on the server and ensure the input and udev directories exist:

```sh
printf 'uinput\nuhid\n' | sudo tee /etc/modules-load.d/kite.conf >/dev/null
sudo modprobe uinput
sudo modprobe uhid
sudo mkdir -p /dev/input
test -d /run/udev/data
```

This full desktop container needs `privileged: true`, host networking and a writable **private** cgroup namespace for systemd and the existing agent budget. It reads `/dev/input` and `/run/udev` for Sunshine hotplug; its Xorg ignores physical server input. Use one container desktop per server: kernel virtual input is shared, so multiple Sunshine desktops on the same server are not isolated from one another. Rootless Docker, Docker Desktop, Windows/macOS servers and ARM64 images are not supported by this deployment. See the [design decision](rfcs/0028-docker-linux-host.md), [systemd container interface](https://systemd.io/CONTAINER_INTERFACE/) and [Docker runtime privileges](https://docs.docker.com/engine/containers/run/#runtime-privilege-and-linux-capabilities).

## Build an image

From the repository root:

```sh
git submodule update --init --recursive
bun install --frozen-lockfile
KITE_IMAGE=registry.example.com/team/kite-host:my-version bun run docker:build
docker push registry.example.com/team/kite-host:my-version
```

The build allowlist excludes local keys, logins, home directories and build outputs. Upstream binary versions and SHA-256 values are pinned in [the Dockerfile](../deploy/docker/Dockerfile). Builds require the initialized Moonlight submodules. CI builds and tests the image for Docker changes; Alpha tags can publish a matching version to `ghcr.io/wenshiqi0/kite/host`. A source build does not require packaging or publishing an APK.

To install additional distribution software, extend the image at build time:

```sh
KITE_EXTRA_APT_PACKAGES='git' bun run docker:build
```

Other vendor applications can be added in a derived Dockerfile. Keep their installers in the image and application profiles in `/home/kite`. Do not bake existing desktop login state or personal credentials into an image.

## Deploy to a server

Create an SSH-backed Docker context using an account that can operate the server's Docker daemon. For private images, run `docker login` on the client before deployment.

```sh
docker context create kite-server --docker host=ssh://ADMIN@SERVER
bun run docker:deploy \
  --context kite-server \
  --image registry.example.com/team/kite-host:my-version \
  --authorized-keys /path/to/kite-phone-authorized_keys
```

The file contains your phone's **public** SSH key from Kite settings, one OpenSSH line per device. Deployment transfers it with `docker cp`, which also works when the daemon is remote. Keys are merged with existing authorization. No private SSH key is copied. On subsequent updates, omit `--authorized-keys` to reuse authorization. Optional `--project`, `--ssh-port`, `--sunshine-port` and `--pull` select the Compose project, listener ports and image refresh. The command waits for service health and prints the SSH fingerprint. A failed deployment retains its volumes and container for diagnosis.

Add the server to the phone with username **kite**, SSH port **2222**, and Sunshine base port **48089** in advanced settings. Compare the reported SSH fingerprint before trusting the new host. Allow these default ports from your private network/VPN:

| Purpose | Protocol | Default port |
| --- | --- | --- |
| SSH | TCP | 2222 |
| Sunshine HTTPS/control | TCP | 48084 |
| Sunshine HTTP/control | TCP | 48089 |
| Sunshine RTSP | TCP | 48110 |
| Video/audio/control | UDP | 48098–48100 |
| Sunshine management (SSH tunnel only) | TCP | 48090 |

For another Sunshine base port `P`, the offsets are TCP `P-5`, `P`, `P+1`, `P+21`, and UDP `P+9` through `P+11`. Host networking preserves the media ports advertised by Sunshine. Keep the server reachable from the phone using the existing LAN/VPN path.

Configure Sunshine's first web login and pairing over an SSH tunnel, using an authorized administration key:

```sh
ssh -p 2222 -L 48090:127.0.0.1:48090 kite@SERVER
```

Open `https://localhost:48090`, create the Sunshine login and submit the phone's pairing PIN. Agent CLIs and their logins are optional additions to the minimal service image. To bundle the fixed Codex CLI, build with `KITE_INSTALL_CODEX=1`, which also installs its Node.js launcher dependency. Then sign in as the container user once:

```sh
docker --context kite-server compose -p kite -f deploy/docker/compose.yaml exec --user kite host codex login --device-auth
```

Provider login is separate from container health. The image does not contain a signed-in account and its startup does not submit a model prompt.

## Configure and operate

The Compose environment supports `TZ`, `KITE_SSH_PORT`, `KITE_SUNSHINE_PORT`, `KITE_DESKTOP_MODE` (`1280x720`, `1920x1080`, `2560x1440`), `KITE_ENCODER` (`software`, `vaapi`, `nvenc`) and `KITE_RENDER_DEVICE`. Default 1080p software encoding works without a GPU. GPU drivers are optional additions: for Intel/AMD, build with `KITE_EXTRA_APT_PACKAGES=mesa-va-drivers`, select `vaapi` and an accessible `/dev/dri/renderD*`; NVIDIA additionally requires the server's matching drivers and NVIDIA Container Toolkit/runtime. GPU paths require verification on the target hardware. [Sunshine configuration](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2configuration.html) describes encoder and device options.

Sunshine configuration is seeded on first boot and then belongs to the persistent user at `/home/kite/.config/sunshine/sunshine.conf`. Change the saved encoder there on an existing deployment. If changing its port, also set `KITE_SUNSHINE_PORT` to match and update the phone; a mismatch fails startup explicitly. Recreate the container to apply environment changes. Avoid changing Compose project names when upgrading: that selects different volumes and identities.

```sh
docker --context kite-server compose -p kite -f deploy/docker/compose.yaml ps
docker --context kite-server compose -p kite -f deploy/docker/compose.yaml exec host journalctl -b -n 100 --no-pager
docker --context kite-server compose -p kite -f deploy/docker/compose.yaml exec --user kite host kite status
docker --context kite-server compose -p kite -f deploy/docker/compose.yaml exec --user kite host kite desktop status
```

The `home` volume keeps workspaces, settings, provider login/history, uploads, Kite identity and Sunshine pairing. The `identity` volume keeps SSH host keys and the system identity. Back up both while the container is stopped. Upgrades replace the image and preserve these volumes. They restart agents and disconnect active clients; this is distinct from Node's in-place live handover. The original host installation remains a separate deployment method.

Health checks verify the SSH/Xorg/user manager, Node, Herdr, Sunshine, desktop session, audio sink and a real delegated memory-limit unit. Inspect `journalctl` for startup failures. `compose down` stops the deployment and retains data; **`compose down --volumes` deletes its persisted identity, pairing, projects and logins**. Do not use it for upgrades.

## Validation

```sh
bun run check
cargo build --locked -p kite-app --example docker_desktop_probe
bun run packages/linux-host/test/qa/docker-host.ts --image kite-host:local \
  --stream-probe target/debug/examples/docker_desktop_probe
```

The opt-in Docker probe creates and removes only a uniquely named project and temporary key. It verifies that no Kite client/panel is installed, checks actual SSH, Node/Herdr, Xfce/IME processes, X11 image output, virtual audio and cgroup enforcement, then recreates the container to check persistence. With `--stream-probe`, a guarded localhost-only native probe also receives Sunshine video/Opus packets and sends mouse/keyboard events to an owned GTK fixture. It uses no personal login and sends no model request. These checks do not establish phone video decoding, audible speaker output or WAN latency.
