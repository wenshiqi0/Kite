# macOS client — 2026-11-02

The native client is implemented on `codex/macos-client`, including main through `3f0abce` (the session-permission and Unicode-input updates). It is a local Mac Alpha build. It has not been published as a signed/notarized Mac release. Linux client UI and macOS host adapters remain pending.

## Delivered

- SwiftUI machine/session sidebar, separate host editing and connection, strict SSH fingerprint confirmation and a Mac-local Keychain identity.
- Shared Core conversation deltas and lazy native history, native Markdown/tool/attachment presentation, model/reasoning/skill selection, approvals/questions and the current session-permission menus.
- A separate desktop panel with Watch/Operate, zoom/pan, native physical keyboard/IME collection, fixed 16ms pointer/wheel and 200ms committed-text batching, optional text clipboard and independent agent pause.
- Apple media C ABI with explicit callback ownership, generation-scoped status, VideoToolbox H.264 decoding, Metal presentation and native Opus playback. Input is gated in Rust as well as the native UI and released on deactivation/disconnect.
- XcodeGen/build/test scripts, an app icon derived from the existing orange four-panel Kite mark, and Mac CI checks.

## Validated locally

Host: Apple Silicon Mac, macOS 26.6.2, Xcode 16.4, deployment target macOS 14. Swift bindings and the Rust static library were generated together.

The 134 shared-client Rust tests pass, as do Clippy with warnings denied, formatting and diff checks. The four offline native tests pass: actual H.264 decoding and restart, Opus decoding across its initial priming delay, Annex B boundaries and aspect-fit/zoom coordinate mapping.

The opt-in fifth native test uses a distinct LAN Linux machine, an isolated Xvfb 1920×1080 display and a separate software-encoded Sunshine instance on port 48200. It uses an independently checked SSH fingerprint and a disposable test key. It confirms a real Codex response, history after SSH reconnect, resumed/archived test sessions and actual Metal presentation through the production SwiftUI `DesktopPane`. The final sustained three-second sample is **33.67 displayed FPS**, with **222 presented frames** in that run. Software encoding and a roughly 30 FPS animated fixture bound this observation; it does not establish a 60 FPS hardware-encoded or WAN result.

The reset remote GTK fixture records **1 left press/release, 1 right press/release, 2 held-motion events and 1 scroll event**, with F8 received and the exact text match for `Kite 远程桌面 29`. Input is sent through the same native media API used by the UI; this is controlled transport/geometry validation, rather than human acceptance of every physical keyboard and IME path. Rust separately verifies disabled-input gating, release ordering, stale frame reports and callback ownership after detach. The numeric record is in [validation.json](validation.json).

The original Linux Sunshine PID **9371** remains running and Kite Node remains active after client disconnect. The QA sessions are archived; only owned fixture processes and their temporary authorization are cleaned up. The installed app is at `~/Applications/Kite.app`, with a persistent local certificate signature and a real `AppIcon.icns`. Keychain access uses the immutable, caller-validated component described in the [authorization acceptance](keychain-update-acceptance.md).

The local network hostname was verified from Avahi's active name and configuration, then resolved through mDNS on the Mac's LAN interface. Surge's active profile was backed up, validated and reloaded; the app process and that local domain match DIRECT for TCP/UDP. No fixed address or Tailscale route is stored in the client host entry.

## Limits and intermediate failures

`cargo test --workspace` on this Mac also enables the Linux host-node feature through workspace feature unification. Three Linux host resource/service tests fail there. They are outside the 134 passing client tests; a green whole-workspace Mac run is not claimed.

The live fixture initially needed a persistent launch without the existing wrapper's parent-death option and a disabled tray. Repeated temporary core directories also changed Sunshine device IDs while reusing a certificate; stable private test state resolves that fixture mismatch. A programmatically created XCTest window needed `isReleasedWhenClosed = false` to avoid double release at teardown. The production panel initially mounted its video surface only after Streaming, while Core requires a presented frame to enter that state. It now mounts during Starting, and the live regression exercises that exact SwiftUI panel. The final five-test run passes. Session initialization also exposed a native send-button gap; the client now waits for the shared agent/composer readiness state.

Mac approvals/questions and permission menus compile against the current shared contracts and their Rust regression checks; a complete manual Mac interaction matrix is pending. Native audio decoding passes, while physical output, Bluetooth and A/V synchronization remain unaccepted. Intel Macs, Developer ID/notarization, long-session native memory profiling, clipboard UI stress, WAN/Tailscale quality and macOS hosting require separate checks. Closing the client preserves host services by design; Mac simultaneous inbound hosting is not implemented by this client stage.
