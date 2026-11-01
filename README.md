# Kite

A native workspace for local and remote coding agents, built around [herdr](https://github.com/herdrdev/herdr). Follow conversations, open a machine's desktop, and operate it when needed.

[Download Alpha packages](https://github.com/wenshiqi0/Kite/releases) · [Set up your host](docs/backend-setup.md) · [Linux desktop host](docs/linux-desktop.md)

## What you can do

- Run a Linux host node with a native control panel and taskbar status; agents and host services continue when the panel closes.
- See connection status on simple host cards, connect or enter a workspace, and edit settings separately.
- Follow Claude / Codex conversations and tool calls, send prompts, and respond to supported terminal approval choices.
- View Codex plan allowance, available reset cards and their earliest expiry alongside model and reasoning selection.
- Keep complete synchronized conversation history, continue saved sessions, and archive them while their tasks keep running. See running, waiting-for-answer and attention states directly in the Android session list.
- Read compact, collapsible tool activity while public progress updates and interim conclusions stay visible; answer model questions with choices or text and see live response feedback.
- Choose a Codex model and reasoning effort, and send images, files and audio attachments. Preview, zoom and save Codex-generated images, with offline access after download. Open image, audio and text attachments in drafts or history.
- Select skills from the composer's plus menu, search their names and descriptions, and remove selected skill chips before sending.
- Keep the live desktop in a draggable window, dock it to the edge, use split view or fullscreen, and continue watching in Android picture-in-picture.
- Switch between Watch and Operate. Zoom and pan, type with the system keyboard, and use a collapsible split-view trackpad or compact fullscreen mouse with a visible arrow pointer. Buttons and scrolling stay aligned as you zoom and pan, with visual and tactile wheel feedback. Pause or resume an agent separately when needed.
- Listen to remote desktop sound, control it with the phone’s media volume, and mute or enable it from the workspace menu.
- Enable automatic text clipboard sharing with one switch in settings, then copy and paste normally between phone and desktop.
- Fit the full desktop to your phone while preserving its source resolution, with responsiveness as the priority.

## Get started

1. Install the ARM64 Android APK from [Releases](https://github.com/wenshiqi0/Kite/releases).
2. Enable the matching [Linux desktop host](docs/linux-desktop.md) with SSH, herdr and Sunshine. Its native panel can close while the background node and agents continue.
3. Add Kite's public key to your host, verify the host fingerprint, and connect. Kite reconnects to the host agent automatically, preparing a default Codex session when needed. Additional project sessions are optional.
4. Open the desktop and pair with Sunshine when prompted. Use the fullscreen orientation button even when phone auto-rotation is off.

The Android app is in Alpha and currently uses a Chinese interface. Codex is connected and validated; Claude sessions can be saved as pending connections. iOS supports the host connection flow; desktop viewing on iOS is still in progress. Android supports remote desktop sound; host local playback stays enabled. Structured history requires herdr session integration; unsupported transcripts remain available as terminal output. Operate keeps the agent running unless you pause it explicitly.

The [desktop product](docs/desktop.md) targets both client and host roles on Linux and macOS, including connections to other desktop machines. The current Linux Alpha delivers the host node and management panel; desktop client functionality is the next stage.

## Documentation

Agent consumers can use `kite docs desktop` for Linux X11 operations or `kite docs` for the short index. CLI, standard MCP and a shared lightweight Skill bundle use the same versioned guides; see [agent integration](docs/agent/harnesses.md). These source features do not imply every published Alpha already contains them.

- [Host setup and troubleshooting](docs/backend-setup.md)
- [Agent notifications and system push setup](docs/notifications.md)
- [Desktop client and host](docs/desktop.md)
- [Implementation status and next stages](docs/implementation-plan.md)
- [Build and development](docs/development.md)
- [Testing](docs/testing.md)
- [Alpha releases and CI](docs/releases.md)
- [Architecture and engineering specification](SPEC.md)

## License

[GPL-3.0-only](LICENSE). Third-party notices are in [NOTICE.txt](NOTICE.txt).
