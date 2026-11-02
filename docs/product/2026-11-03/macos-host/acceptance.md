# Mac local host — 2026-11-03

## Delivered

The Mac app includes a native local-host window and menu-bar entry. Swift invokes the bundled structured Node CLI; Rust owns installation, service policy and launchd lifecycle. Bun and the display-metadata helper ship inside the signed `Kite Host.app`. Runtime permissions attach to a fixed bundle identity and install path, not each component hash directory. Herdr and the official Developer-ID-signed Sunshine Mac app are installed on the validation machine.

Existing Node API 1, backend 5, harness 2, SSH authentication, session catalog and provider history contracts are retained. Mac reports unavailable Linux-only resource/pause/X11 capabilities. New Mac Codex threads explicitly retain the existing legacy-history contract and are named/persisted before their TUI resumes; initialization sends no model turn. No global Codex configuration or credentials are modified.

## Accepted evidence

- Built-in **Codex Computer Use**, selecting `/Users/wenshiqi/Applications/Kite.app` by full path, opened **本机服务**, clicked install/enable and observed the real Node PID. It then clicked update/restart; Node changed from PID 15741 to 6497 while Herdr retained PID 15738. A nested launchd `state=active` field initially obscured the actual running state; the parser now uses the job's first state and has a regression.
- Quit Kite using its visible menu. The original Node PID 15741 and Herdr PID 15738 remained alive. Subsequent component updates retained the same node ID, catalog and provider owner. No Kite Keychain authorization flow was entered. Two host bundles with different code/resource hashes retain the same pinned certificate/bundle-ID designated requirement; runtime installation also retains a fixed app path. This verifies identity stability, not a fresh TCC grant while locked.
- An independent Linux machine authenticated over LAN SSH to this actual Mac, using its independently pinned host key and an owned temporary source-restricted SSH key. It read real Node/harness state, accessed an already initialized Mac Codex session, sent a no-tool validation prompt and received **MAC_HOST_LAN_OK**. Reconnection recovered the same provider thread and the actual reply in history.
- The host panel reports the system's published name **wen-mac.local**, user **wenshiqi**, SSH port **22**. This name was read from macOS; the LAN SSH probe used an explicit test transport address. Android hostname resolution is not claimed by that probe.
- Native display metadata reports CGDisplayID 3, **1920×1080** pixels. Geometry comes from the same CoreGraphics display-mode pixel dimensions as the verified Sunshine implementation; invalid/inactive targets are rejected. This is metadata evidence, not decoded-video evidence.
- Surge's active Default profile now matches Sunshine UDP to **DIRECT**, rather than its prior FINAL proxy rule. Kite Node and the local Mac hostname are direct as well. The profile was backed up privately, validated and reloaded; other rules and proxy credentials were preserved.
- Mac Bun checks after the Alpha 67 mainline merge: **60 passed, 3 Linux-specific tests skipped**. Rust client/host/store checks: **158 passed**; Clippy with warnings denied and formatting passed. Four native decoder/geometry tests passed; the opt-in live media fixture was skipped in this run. Linux full-workspace and Android packaging checks run separately in tagged CI. Mainline session deletion is adapted to Darwin PID/start-time/ancestry checks, including detached local descendants; the existing deletion fixtures pass on Mac. Linux retains its existing group/cgroup path.

The completed validation conversation is retained for inspection. Test authorizations are removed after the final LAN checks; no device private keys or local authorization records are committed or uploaded.

## Boundaries

Built-in Computer Use subsequently reported **Mac locked; automatic unlock failed**. A user-unlock request is pending. No Tactero or alternate UI driver was used. Fresh background Codex session creation still times out, while the same SDK starts under the development app. TCC logs record a denied AllFiles preflight attributed to the earlier generation-path node; this establishes the identity problem but does not by itself prove the entire timeout cause. The fixed signed host app has been installed; a fresh grant and cold-start recheck await the unlocked Mac. Mac Sunshine screen-recording/accessibility/audio permission setup, actual incoming video/input/audio, simultaneous three-device GUI acceptance and handset installation remain unaccepted. An online management endpoint is not a media pass.

Mac kernel-enforced aggregate agent budgets, process pause and X11 desktop MCP actions are intentionally unavailable. WAN/Tailscale quality, Intel Macs and Developer ID/notarized Mac distribution remain separate work. The Android Alpha 69 delivery is published only after tagged CI succeeds; its signed Release assets and manifest are authoritative.

## Sources

- [Mac host setup](../../../macos-host.md) and [RFC 0029](../../../rfcs/0029-macos-host.md)
- [Mac host policy](../../../../crates/kite-app/src/host_node/macos.rs), [launchd state parsing](../../../../crates/kite-app/src/host_node/launchd.rs)
- [Darwin process facts](../../../../packages/runtime/src/darwin.ts), [private socket validation](../../../../packages/runtime/src/socket.ts), [Mac Codex initialization](../../../../packages/providers/src/codex.ts)
- [Capture target verification](../../../../packages/platform/src/captured-display.ts), [native pixel metadata](../../../../macos/HostPlatform/main.swift), [client geometry](../../../../crates/kite-stream/src/display.rs)
