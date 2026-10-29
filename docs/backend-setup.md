# Host setup

Kite connects to services on your own machine. SSH carries herdr, Codex chat and Sunshine control requests; Sunshine sends desktop video and input over your private network. Rich Codex chat uses a private, persistent app-server for each Kite pane. The APK installs a small Python host adapter in `~/.local/lib/kite/backend-v1.py`; it does not expose another TCP port or change the global Codex daemon.

This guide covers the current Android Alpha's supported host path: **Linux with an active X11 desktop**, with SSH, herdr and Sunshine running as the same user. Automatic capture-size discovery currently requires Linux `/proc`, `ss` and `xrandr`. Wayland, Windows and macOS host discovery are not implemented in this Alpha, even where upstream herdr or Sunshine support them.

## First connection from your phone

1. Open **Host settings** (主机设置), enter an IP / hostname reachable **from the phone**, the SSH port and your host username. A laptop SSH alias such as `wen.linux` is not automatically available on Android. Keep the phone on the host LAN, or configure a reachable private VPN path.
2. Copy the phone's public key and authorize it on the host as described below. Each phone has its own device key.
3. Choose **Connect host** (连接主机). The form stays visible through SSH authentication, herdr discovery and API checks. Verify the host fingerprint when prompted. On failure, the reason is shown at the top; **Copy diagnostics** in the settings toolbar copies connection stages and public fingerprints, without private keys or Sunshine passwords.
4. Kite automatically reuses the host agent, preferring its persistent `kite` session. If no agent exists, it creates a `Kite` workspace in the host user's home directory and starts Codex. Codex must be installed and signed in on the host once; afterward, reconnecting the phone reuses the same background agent. Closing Kite or disconnecting SSH does not stop that process. No model task is submitted automatically. Kite starts a pane-owned Codex app-server and attaches the herdr TUI with `codex --remote unix://...`. The phone submits structured turns to that same server; it never starts a second owner for an existing thread. The server survives SSH disconnects, and herdr retains a resume command that can recreate its private listener. Interactive update prompts are disabled for this invocation; Codex updates are managed on the host. Other Codex sessions and global configuration are unchanged. **New session** is an optional action for another provider or project directory, not a prerequisite for messaging.
5. For first-run choices, open **Terminal output** (终端输出) and use its arrow / Enter / Esc controls. A waiting agent may need login, workspace trust or another explicit choice before it can accept chat. Failed starts retain their workspace for inspection; an uncertain start is not replayed automatically.
6. Open the remote desktop and pair with Sunshine. **Watch / Operate** (只看 / 操作) controls whether input reaches the desktop. The agent continues running independently.

## Rich chat requirements and upgrading an existing Alpha host

Install **Codex CLI 0.164.0 or newer** and **Python 3** under the SSH user, and sign in to Codex on the host. [Codex's app-server transport](https://learn.chatgpt.com/docs/app-server) is experimental; Kite currently ships this integration as Alpha. The host adapter uses the documented Unix WebSocket transport through SSH. Socket directories are private to the host user, and no API key or authentication token is copied to the phone.

New Kite Codex panes are prepared automatically. To upgrade a pane created by an older Alpha, first connect the new APK so its matching host adapter is installed. Find the pane ID using `herdr agent list`; when the agent is idle, run:

```bash
python3 ~/.local/lib/kite/backend-v1.py upgrade \
  --pane YOUR_PANE_ID --herdr "$(command -v herdr)"
```

The command verifies the idle in-process Codex TUI, stops that exact process and attaches a TUI to the new pane-owned server with the **same thread UUID**. It refuses active turns or unfamiliar process layouts. Do not repeatedly run it after an uncertain result; inspect the pane first. Custom agents and other providers are not migrated automatically. Host records, SSH keys, trust, Sunshine pairing and provider history are retained. A host adapter update must accompany the APK; no legacy Codex chat endpoint is maintained.

The chat composer reads the real `model/list` catalog and shows each model's supported reasoning efforts. Choices are stored per host and conversation; `turn/start` receives both values explicitly. These controls and attachment uploads currently require a Kite Codex pane; Claude terminal chat remains text only.

Use **+ → Image / File / Audio** to choose attachments with Android's system picker. Each message accepts up to five files, at most 25 MiB each. Uploads stream through SSH, verify SHA-256 and commit atomically under `~/.local/share/kite/uploads/`; failed uploads remain available to retry or remove. Files are retained on the host for the conversation. Images use Codex's native image input. Models advertising audio input use native audio; otherwise audio is a file for the agent's tools to read, and the UI says so. The microphone button performs **speech to text**, a separate feature.

Approvals remain owned by the attached TUI. Kite reads the actual choices and checks their session/state before sending the corresponding keys. The structured chat connection never grants approval automatically. Unknown interactions can still be handled through **Terminal output**.

## 1. Prepare SSH and a desktop session

On Debian / Ubuntu, install the SSH server and display-query tools if needed:

```bash
sudo apt install openssh-server iproute2 x11-xserver-utils
sudo systemctl enable --now ssh
```

Log into an X11 desktop, physical or virtual. Sunshine must capture an existing desktop; connecting Kite does not create one. From a terminal **inside that desktop**, check its environment and active output:

```bash
printf 'DISPLAY=%s\nXAUTHORITY=%s\n' "$DISPLAY" "$XAUTHORITY"
xrandr --query
```

Record the actual display and output name. Examples include `:0` with `DP-1`, or an xrdp session such as `:10` with `rdp0`. Use your values throughout this guide.

## 2. Install and start herdr

Use the [official herdr installer](https://github.com/herdrdev/herdr#install) or an [official release binary](https://github.com/herdrdev/herdr/releases). The current Kite protocol was verified against **herdr 0.9.5**.

Confirm the executable and API bridge:

```bash
herdr --version
herdr remote-api-bridge --check
```

The bridge check must report `herdr-api-bridge-v1`. Start herdr in your project directory and run your agents there:

```bash
cd /path/to/your/project
herdr
```

Detach with `Ctrl+B`, then `Q`; the background server keeps the session running. Check it with `herdr status server`. The [upstream quick start](https://herdr.dev/docs/quick-start/) explains workspace and agent setup.

For structured Claude / Codex conversation history, enable the corresponding [official herdr integration](https://herdr.dev/docs/integrations/) under the host account that runs the agent:

```bash
herdr integration status
herdr integration install claude
herdr integration install codex
```

Install only the integrations for agents you use, then start a new agent session inside herdr. These commands update those agents' user configuration with herdr's session hooks. Kite uses the reported session identity to read the selected Claude JSONL under `~/.claude/projects`, or Codex JSONL under `~/.codex/sessions` (`CLAUDE_CONFIG_DIR` / `CODEX_HOME` are respected when present in the SSH environment). Without an available supported session record, Kite labels the content as terminal output. It does not infer chat speakers from a terminal screenshot.

Kite searches the SSH user's normal PATH and common install locations, including `~/.local/bin/herdr`. If detection fails, enter the executable's absolute path in Kite's advanced host settings. No separate herdr TCP endpoint needs to be exposed.

## 3. Install and configure Sunshine

Install the package for your distribution from [Sunshine's official releases](https://github.com/LizardByte/Sunshine/releases), following its [installation guide](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2getting__started.html). Kite's current pairing and input flow was verified against **Sunshine v2026.914.233613**.

Open Sunshine's local web UI at `https://localhost:47990` and create its management account. Keep an application named **Desktop** in its application list; Kite selects that application when opening a desktop stream.

Edit `~/.config/sunshine/sunshine.conf` or use the corresponding web UI settings. For the current Linux X11 path, set `capture` explicitly to `x11`. For example:

```ini
sunshine_name = My development desktop
capture = x11
output_name = DP-1
address_family = ipv4
bind_address = 192.168.1.50
port = 47989
upnp = disabled
origin_web_ui_allowed = lan
dd_configuration_option = disabled
```

Replace `DP-1` with the output shown by `xrandr`, and `192.168.1.50` with the host's reachable private address. A numeric output index is also supported, but a connector name makes the selection clearer. See [Sunshine's configuration reference](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2configuration.html) for these options and GPU-specific encoders. On a VAAPI-capable AMD / Intel host, `encoder = vaapi` and a valid `adapter_name`, such as `/dev/dri/renderD128`, can select hardware encoding; use a device your desktop user can access.

Start Sunshine as that desktop user:

```bash
systemctl --user enable --now app-dev.lizardbyte.app.Sunshine
systemctl --user status app-dev.lizardbyte.app.Sunshine
```

Sunshine also provides the `sunshine.service` alias. Ensure the service has the desktop's actual `DISPLAY` and `XAUTHORITY`, especially when starting it over SSH. If necessary, use `systemctl --user edit app-dev.lizardbyte.app.Sunshine` to set them explicitly:

```ini
[Service]
Environment=DISPLAY=:0
Environment=XAUTHORITY=/home/alice/.Xauthority
```

Use your own display, user and authority path, then restart the user service. A successful service start alone does not prove capture: Sunshine's log must show the intended output, and Kite must actually present frames.

## 4. Connect Kite's device key

Open Kite’s agent menu, choose **Host settings** (主机设置), and copy the public key. Add that line to the host user's `~/.ssh/authorized_keys`, preserving existing entries:

```text
ssh-ed25519 AAAA... kite-device
```

The file should belong to the SSH user, with `~/.ssh` mode `700` and `authorized_keys` mode `600`. Verify the host fingerprint independently:

```bash
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

Enter the host address, SSH port and username in Kite. Compare the displayed host fingerprint before trusting it. Changed fingerprints are rejected until the saved trust is explicitly reviewed.

SSH must allow both exec channels and `direct-tcpip` forwarding. If you use OpenSSH's `restrict` key option, re-enable forwarding for Kite's control channel:

```text
restrict,port-forwarding ssh-ed25519 AAAA... kite-device
```

Kite does not require an interactive PTY, but a forced command limited to one herdr operation will prevent its other host checks. Confirm any host-level `AllowTcpForwarding` or `PermitOpen` policy also permits the Sunshine endpoint.

## 5. Pair and open the desktop

After herdr connects, choose an agent in the menu. Use the **More** menu to open the remote desktop (打开远程桌面). Sunshine's address defaults to the SSH host; advanced settings allow a separate reachable address and base port.

On first use, Kite shows a PIN and a unique client name. In Sunshine's management UI, choose the matching pending request and submit that PIN. Kite saves the client identity and server certificate, so reopening the desktop normally does not require pairing again.

The stream starts in **Watch** (只看). Choose **Operate** (操作) to send input. Both modes support local zoom / pan without changing the host desktop resolution. Fullscreen provides an explicit portrait / landscape button; entering fullscreen requests landscape even if system auto-rotation is off.

In Operate, the input button switches between **Trackpad** (触控板) and **Direct touch** (直接触控). Trackpad uses one finger to move the pointer, a tap for left-click, double-tap-and-drag or hold-and-drag for dragging, a two-finger tap for right-click and parallel two-finger movement for scrolling. A pinch zooms locally without also clicking or scrolling the host. Direct touch maps the visible point through the inverse viewport transform to source desktop pixels; its two-finger gestures zoom and pan. Letterbox touches do not click the desktop. **Move view** (移画面) explicitly switches one-finger gestures to local panning while keeping Operate selected. The toolbar also provides left / right mouse buttons, keyboard shortcuts, arrows, Unicode text entry and clipboard paste.

The desktop initially floats above chat. Its controls switch to split view, fullscreen or an edge tab; the split divider is draggable. Leaving the app while video is playing enters Android system picture-in-picture when supported. Returning restores the same desktop session.

**Takeover does not pause the agent.** The More menu provides a separate pause / resume action for verified Linux Claude / Codex foreground processes. Kite stores their process identity before pausing so a restart can offer resume. This pauses execution, not an agent protocol or a model request: upstream requests may time out while stopped. Resume the tracked process before sending more chat or approval input.

### Optional automatic PIN confirmation

Manual pairing does not require credentials in Kite. For automatic confirmation, the SSH user can maintain a private file at `~/.config/sunshine/admin-credentials.json`:

```json
{"username": "your-sunshine-account", "password": "your-management-password"}
```

Use mode `600`. This is a **Kite convenience file**, not Sunshine's hashed credential store, and its values must match the account created in the web UI. Kite reads it only over the verified SSH connection when you choose automatic confirmation; it is not copied into app snapshots or public host storage.

## Networking

Use a LAN or a private VPN such as Tailscale. Both SSH control and the direct video path must be reachable from the phone. A working SSH connection does not prove UDP video connectivity.

With the default Sunshine base port, the relevant services are:

| Port | Protocol | Purpose in Kite |
| --- | --- | --- |
| 22 | TCP | SSH authentication, herdr and tunneled control |
| 47984 / 47989 | TCP | Sunshine GameStream control through SSH |
| 47990 | TCP | Sunshine management UI / PIN confirmation |
| 48010 | TCP | Direct RTSP session setup |
| 47998 / 47999 / 48000 | UDP | Sunshine media and real-time control transport |

Changing the base port shifts the Sunshine port family. The current app does not play audio even though the protocol establishes its transport. Keep UPnP disabled and restrict host firewall access to the intended private network. If using Tailscale, bind Sunshine to the host's reachable Tailscale address, or configure the relevant private interfaces and firewall deliberately; a LAN-only bind does not also listen on the Tailscale address.

## Resolution and responsiveness

A 1920×1080 host desktop is a practical starting point. Kite preserves that source size and aspect-fits it on the phone; rotating the phone does not request a different desktop mode. It prefers a 60fps video request, permits a small proportional reduction to maintain decoder frame-rate support, and falls back to 30fps when needed. Requested FPS is separate from host refresh rate and measured presentation rate. Network-driven dynamic resolution switching is not implemented yet.

Set the host mode using its desktop settings, or an available mode from `xrandr`:

```bash
xrandr --output DP-1 --mode 1920x1080
```

xrdp may reject a normal `xrandr` resize; use the RDP client's display settings for that session. After changing the desktop mode, restart Sunshine and reconnect the stream. In the tested X11 version, reopening video alone retained the old XTEST coordinate mapping, while restarting Sunshine corrected it without restarting the desktop applications.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| herdr missing | SSH user's PATH or the advanced absolute executable path |
| No agents listed | Run agents inside the same user's running herdr session |
| Size discovery fails | Same-user Sunshine, explicit `capture = x11`, correct service display / authority, working `ss` and `xrandr` |
| Desktop application missing | Sunshine has an application named exactly `Desktop` |
| Pairing fails | Matching client name / PIN, current management account, SSH forwarding allowed |
| Video connects but no frames appear | Private TCP / UDP reachability, active X11 output and a working Sunshine encoder |
| Clicks are offset after resizing | Restart Sunshine so its input mapping matches the current mode |
| Keyboard / mouse fails | Takeover enabled, correct desktop focus, and Sunshine's distribution-specific input setup |

For developer-side fixtures and what counts as runtime acceptance, see [Testing](testing.md).
