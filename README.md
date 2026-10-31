# Kite

A native mobile companion for [herdr](https://github.com/herdrdev/herdr). Follow your coding agents, open their desktop, and take over when they need a hand.

[Download Android Alpha](https://github.com/wenshiqi0/Kite/releases) · [Set up your host](docs/backend-setup.md)

## What you can do

- See connection status on simple host cards, connect or enter a workspace, and edit settings separately.
- Follow Claude / Codex conversations and tool calls, send prompts, and respond to supported terminal approval choices.
- Keep complete synchronized conversation history, continue saved sessions, and archive them while their tasks keep running.
- Read compact, collapsible tool activity; answer model questions with choices or text and see live response feedback.
- Choose a Codex model and reasoning effort, and send images, files and audio attachments.
- Keep the live desktop in a draggable window, dock it to the edge, use split view or fullscreen, and continue watching in Android picture-in-picture.
- Switch between Watch and Operate. Zoom and pan, type with the system keyboard, and use a collapsible split-view trackpad or compact fullscreen mouse with a visible arrow pointer. Buttons and scrolling stay aligned as you zoom and pan, with visual and tactile wheel feedback. Pause or resume an agent separately when needed.
- Enable automatic text clipboard sharing with one switch in settings, then copy and paste normally between phone and desktop.
- Fit the full desktop to your phone while preserving its source resolution, with responsiveness as the priority.

## Get started

1. Install the ARM64 Android APK from [Releases](https://github.com/wenshiqi0/Kite/releases).
2. Prepare a Linux X11 host with SSH, herdr and Sunshine using the [host setup guide](docs/backend-setup.md).
3. Add Kite's public key to your host, verify the host fingerprint, and connect. Kite reconnects to the host agent automatically, preparing a default Codex session when needed. Additional project sessions are optional.
4. Open the desktop and pair with Sunshine when prompted. Use the fullscreen orientation button even when phone auto-rotation is off.

The Android app is in Alpha and currently uses a Chinese interface. Codex is connected and validated; Claude sessions can be saved as pending connections. iOS supports the host connection flow; desktop viewing on iOS is still in progress. Desktop audio playback is not available yet. Structured history requires herdr session integration; unsupported transcripts remain available as terminal output. Operate keeps the agent running unless you pause it explicitly.

## Documentation

- [Host setup and troubleshooting](docs/backend-setup.md)
- [Implementation status and next stages](docs/implementation-plan.md)
- [Build and development](docs/development.md)
- [Testing](docs/testing.md)
- [Alpha releases and CI](docs/releases.md)
- [Architecture and engineering specification](SPEC.md)

## License

[GPL-3.0-only](LICENSE). Third-party notices are in [NOTICE.txt](NOTICE.txt).
