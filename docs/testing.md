# Testing

## Offline checks

The domain and foundation suites use in-process SSH / HTTP fixtures and do not require an external host:

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
./android/gradlew -p android :app:assembleDebug :app:lintDebug :app:testDebugUnitTest
```

Android JVM tests now cover the pure source-to-viewport transform, inverse touch mapping, letterbox rejection, pinch focus, pan limits and orientation preservation. CI runs `:app:testReleaseUnitTest` with Release assembly and lint. A future `NO-SOURCE` result must not be counted as a passing test suite.

## Desktop acceptance

A host check, a listening Sunshine service or receipt of encoded packets is insufficient. Validate all of these on the same build and host:

- The existing device SSH key, host records and trust records survive upgrade.
- SSH and the herdr API remain connected after the initial check.
- Pairing succeeds, and the saved client identity is reused on reopen.
- MediaCodec actually presents frames, with source and video dimensions recorded.
- Phone rotation preserves the source resolution and aspect ratio.
- View-only mode sends no input; takeover clicks, scrolling, keys and UTF-8 text reach a controlled host window.
- A host-side response is visible in the returned video, not just in an input-send result.
- Closing the desktop retains SSH; background and foreground transitions pause and resume video correctly.
- Disconnects and repeated connection failures do not crash the foreground service.

The earlier desktop baseline used an ARM64 Android API 36 emulator, a Linux X11 desktop, herdr 0.9.5 and Sunshine v2026.914.233613 over LAN. A 1920×1080 source was decoded at 1920×1080, with a 60fps request; the host refreshed at 50Hz and a SurfaceFlinger sample showed approximately 50 presented frames per second. This is a historical emulator measurement, not a fresh Alpha 11 frame-rate result or physical-phone/WAN acceptance. Source changes, build checks, host state and user-perceived latency are separate evidence. The later Alpha 11 interaction and released-APK checks are recorded below.

## Chat and desktop workspace acceptance

Android Alpha 2 was exercised on the same API 36 ARM64 emulator and real Linux SSH / herdr / Sunshine services. A clearly labelled Claude-format test process supplied deterministic history, approval choices and heartbeats; no model was invoked and no example tool command was executed. Checks cover:

- Structured messages, tool cards and UTF-8 prompt submission through the selected herdr pane.
- Allow / reject choices sent once, with stale session / state / prompt rejection in Rust tests.
- Floating-window movement, edge docking, portrait split and landscape watch / takeover.
- A host click during takeover while agent heartbeats continue.
- Explicit process pause while video continues; app restart followed by identity-checked resume.
- Remote Unicode entry, trackpad movement and Android PiP return.

This validates the client transport and product flows with controlled data. It does not establish real-model completion, voice-recognition quality, physical-phone latency or WAN performance. See the [product acceptance record](product/2026-10-29/acceptance.md) for the recorded result and screenshots.

## Alpha 5 phone feedback regression

The phone usability fixes have dedicated source-pixel viewport tests, plus real emulator gestures injected through the independent `ui-probe` package. Runtime acceptance checks operation-mode pinch without host click / wheel events, click targeting after zoom + pan, trackpad right-click / scroll, orientation controls with system auto-rotation off, automatic default-agent preparation and same-session reconnect, connection failures, terminal keys and PiP decoder reuse. See the [phone feedback record](product/2026-10-29/phone-fixes.md).

The herdr SSH fixture also checks workspace creation followed by `agent.start` in the returned public pane, preservation of project-directory text and reporting a failed create once without starting an agent. Provider `interactive_ready` is a **managed-launch** flag; it is not a gate for ordinary agents started directly in herdr.

## Alpha 11 hybrid input and clipboard

The [Alpha 11 acceptance record](product/2026-10-30/hybrid-input/acceptance.md) covers one-switch automatic text clipboard sharing, direct touch combined with held virtual mouse buttons, wheel events, zoomed source-pixel targeting, sidebar dismissal and richer Markdown control labels. The herdr-core restoration experiment is [recorded separately](product/2026-10-30/herdr-core-migration.md); it does not establish a production migration or desktop-client completion.

This delivery passed 60 Rust tests, 10 host-adapter tests, 4 publisher tests, 13 Android JVM tests and 6 native API 36 tests. The [23 recorded runtime cases](product/2026-10-30/hybrid-input/results.json) include checks on the exact public Release APK, with [source/checksum/CI evidence](product/2026-10-30/hybrid-input/published-release.json). Its emulator update retained SSH identity, desktop pairing and the clipboard preference; the released bytes passed automatic clipboard sharing in both directions. Alpha 11 physical-phone responsiveness and WAN performance remain pending.

## Optional integration tools

| Tool | Scope |
| --- | --- |
| `crates/kite-app/examples/desktop_probe.rs` | Real SSH, herdr API, pairing and encoded transport; no decoder assertion |
| `crates/kite-app/examples/product_agent_fixture.py` | Controlled Claude-format transcript, choices and process-heartbeat fixture; no model or command execution |
| `crates/kite-app/examples/desktop_fixture.py` | Linux GTK3 animated target; records input counters and one exact test-text match |
| `android/ui-probe` | Independent test-only accessibility package for entering Unicode in the app's text composer |

The host fixture is not a product runtime dependency. It restores the prior focus on exit and does not change the display mode. Input testing must target that live fixture. Remove temporary SSH authorizations and test helper apps afterward; keep the user's desktop session and normal Sunshine service.

A screenshot-based click round trip includes input-dispatch and screenshot overhead. Do not report it as a pure touch-to-photon latency measurement. Likewise, a requested 60fps mode is not proof of sustained 60fps presentation.

## Compact controls and paged history

Alpha 12 adds tests for fixed-window IME batching, Unicode deletion/chunking, portrait geometry budgets, zoomed one-finger panning, IME composition/key ordering, history cursor traversal and invalidation, cross-page tool merging, bounded bidirectional windows and non-destructive host saving. Chat grouping keys use a separate content revision; video telemetry does not invalidate them.

The opt-in `LiveDesktopInputTest` verifies the real IME → 200 ms scheduler → JNI → Sunshine path against an isolated GTK fixture. It skips unless explicitly enabled with `qa_live=true` and an owned `qa_session`. Its fixed host/port guards must not be relaxed to target a production desktop. Keep its authorization and fixture data outside the repository, remove only owned test records, and retain existing host services and phone keys.

Controlled runtime evidence is recorded separately from host/JVM checks. Android API 36 emulator behavior does not establish physical-phone latency or WAN acceptance.

The completed [Alpha 12 acceptance](product/2026-10-31/compact-controls/acceptance.md) links native screenshots, the public APK checksum, passing CI runs, in-place install/retention checks and owned-resource cleanup.


## Complete conversations and local rendering

The [Alpha 13 revision](product/2026-10-31/complete-transcript/acceptance.md) replaces UI-driven history pagination with independent full synchronization, complete per-session caches and versioned message edits. Native tests use 10,000 records and a 3MiB tool result, inspect actual rich-text view allocation and verify that telemetry does not replace message objects. The owned-source-unavailable check verifies local scrolling after a fresh cache reopen; the append check compares visible message text and pixel bounds. Large tool details use bounded virtual text chunks.

The optional long-transcript test accepts qa_live, a strictly owned qa_session and qa_records (10000 by default). Never point these opt-in tests at a user's desktop. They do not establish physical-phone/WAN performance.
