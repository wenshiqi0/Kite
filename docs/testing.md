# Testing

## Agent notifications

Rust notification tests cover transition provenance, fast Codex turns, persistence/deduplication, cursors, private token rotation/revocation and independent storage failures. Backend tests check lifecycle fact privacy and bounded FCM responses/retry without network calls. `AgentNotificationTest` uses the real Android NotificationManager and requires granted POST_NOTIFICATIONS; cancellation assertions await Android's asynchronous propagation. `LiveAgentNotificationTest` is opt-in (`qa_notifications=true`) and guarded to the owned emulator SSH profile and exact Alpha 28 QA cwd. It creates one disposable Codex conversation and one tool-free turn, checks a durable completion event, actual system posting and PendingIntent return, then prepares an existing event for an external cold-start tap. Remove temporary authorization and only those owned resources afterward.

The exact public-APK results and the distinction between connected notifications, cold-start navigation and actual FCM process-reclamation delivery are recorded in [Alpha 28 acceptance](product/2026-11-01/agent-notifications/acceptance.md). Full system-push acceptance requires a real configured project/device, not an injected callback.

## Chat allowance and input refinements

`ChatTailMarginTest` checks native scroll thresholds. Rust `usage` tests cover provider facts, missing data, account/notification races and stale results. `WorkspaceRefinementPresentationTest` renders synthetic allowance cards and checks the real workspace's scroll affordance, editor bounds and native text changes; its history case requires `qa_native_history=true`, injects only a native display mirror and restores it from Rust afterward.

`LiveQuotaAndDesktopTest` is opt-in (`qa_live_quota=true`). It requires the owned `10.0.2.2:22223` / Sunshine `48200` profile and a cwd under `/home/wenshiqi/.cache/kite-alpha20-qa-20261101/`. It creates an owned empty Codex session, reads quota, checks model-picker placement and stops desktop setup from the workspace menu while retaining chat. Set `qa_video_required=true` only with an isolated paired Sunshine fixture to additionally require actual frames. Records distinguish setup cancellation from active-video acceptance. No model turn is submitted. The guarded `qa_phase=public-key` phase exports only the validation device's public key; temporary authorization stays outside the repository and is removed afterward.

## Sustained video and continuous input

`StreamingCadenceTest` is opt-in (`qa_stream_perf=true`, `qa_seconds=12`, range 10–30). It requires the owned `10.0.2.2:22223` / Sunshine `48200` host and an isolated 1920×1080 moving scene. It requires actual presented frames, warms the decoder, samples receive/render counters across fresh core statistics, and records UI-frame cadence plus available decoder metrics. It then sends a 60-sample continuous fullscreen pointer sweep through VideoSurface → DesktopInput → JNI; compare the independently recorded host motion count and span to detect delivery delayed until gesture end. It submits no model prompt and closes its desktop on success. Keep fixture routing, scene, public APKs and warmup identical for A/B results; the old APK has no new decoder metrics. Test View dispatch and host arrival timing are not physical finger-to-photon latency. See [performance acceptance](product/2026-11-01/streaming-performance/acceptance.md).

## New sessions and interrupted desktops

`NewSessionRegressionTest` is opt-in (`qa_new_session=true`) and requires the owned QA host plus the exact owned Alpha 24 project path. Baseline mode sends no prompt; final mode creates two disposable Codex sessions, sends one tool-free marker prompt, checks the first reply and retained history after reconnect, and detects false sync failures, automatic terminal presentation and cross-session content bleed. It writes owned session IDs privately for precise cleanup.

`DesktopRecoveryTest` is opt-in (`qa_desktop_recovery=true`) on the same isolated host/Sunshine port guard. It requires three actual 1080p video cycles, a usable idle/reconnect presentation, and an explicit failed state after a controller stops only the owned Sunshine instance. Production desktop services must not be targeted. See [acceptance](product/2026-11-01/new-session-and-recovery/acceptance.md).

`AudioStreamTest` is opt-in (`qa_audio_stream=true`) with the same guarded QA host. It requires an isolated PulseAudio server and a non-silent synthetic test signal, then checks real Opus decoding, AudioTrack playback-head progress, menu mute/resume, transient focus loss/return and audio-thread release across three 1080p stream cycles. It does not record microphone or system audio and does not prove physical speaker/Bluetooth audibility or WAN A/V synchronization. The private audio server and tone generator are cleaned up separately from user audio services.

## Linux status UI

On Linux with Python GI, GTK 3, Xvfb and a session D-Bus:

```sh
cargo test -p kite-app --features host-node host_node:: --locked
python3 -m unittest discover -s desktop/linux -p 'test_*.py'
xvfb-run -a dbus-run-session -- /usr/bin/python3 desktop/linux/qa/tray_probe.py
xvfb-run -a dbus-run-session -- /usr/bin/python3 desktop/linux/qa/operations_probe.py
xvfb-run -a dbus-run-session -- /usr/bin/python3 desktop/linux/qa/upgrade_probe.py
```

The native probe uses an isolated CLI fixture. It verifies health transitions, stale-state clearing, timeout/recovery, menu actions and repeated launches. Without a tray it requires an automatically accessible window. On a supported Xfce/X11 desktop, run the probe in a private session bus with `--require-tray --evidence /path/to/owned-output` to additionally require a real embedded icon, hide/reopen the same window and record fixture screenshots. The probe does not stop real Herdr or Sunshine services. Trusted CI runs the missing-tray probe; actual Xfce results are recorded in [taskbar acceptance](product/2026-10-31/linux-taskbar/acceptance.md).

`operations_probe.py` checks real GTK widgets while status and actions overlap: queued/deduplicated actions, health polling during startup, failures retained across healthy polls, successful retry, stable service widgets, skipped hidden-window rendering and three/fifteen-second foreground/background scheduling. Its optional `--evidence /owned/output` captures the failure and recovery panels. `upgrade_probe.py` uses two owned package directories on a private session bus, replaces the launcher and verifies a visible original GUI remains open until normal exit, after which the next launch loads the new release. Run it only on an isolated display and private bus. Installer unit tests cover interrupted-upgrade retry, running-executable verification, custom sockets, retained configuration/autostart preferences and deferred GUI updates and explicit first-migration refusal. Node tests cover batched cached probes, explicit refresh, service aliases, post-start cache invalidation and inspection timeout diagnostics.

For live upgrades, a **real systemd user manager** is required:

```sh
cargo build -p kite-node --locked
python3 desktop/linux/qa/live_upgrade_probe.py --binary target/debug/kite-node --evidence /owned/output/live-upgrade.json
```

This probe installs into a temporary user tree, routes manager operations only to uniquely named `kite-qa-upgrade-*` socket/service units and uses the actual installer and generated units. It continuously verifies duplex framed data across an executable replacement, a blocked exactly-once write, the same controlled detached agent PID/heartbeat, new connections on the new generation, old-generation drain, incompatible/broken candidates, corrupt persistent state, MAINPID notification failure/rollback, replacement adoption and automatic recovery after its death while old streams remain open, a notification transition without duplicate writers and concurrent push-registration retention. The interpreter supplies only owned notification facts; no real model, catalogue, FCM credentials, Herdr or Sunshine service is used. All owned processes/units are removed. CI runs it only when a user manager is available and explicitly reports a skip otherwise. This is transport/control acceptance, not actual mobile video, physical network latency or real-model generation acceptance.

## Offline checks

Agent docs/desktop acceptance uses an owned Node and X11 fixture:

```sh
cargo build -p kite-node --locked
xvfb-run -a dbus-run-session -- /usr/bin/python3 desktop/linux/qa/agent_desktop_probe.py --binary target/debug/kite-node
uv run --no-project --with tiktoken python desktop/linux/qa/docs_budget.py --binary target/debug/kite-node
python3 desktop/linux/qa/skill_consumers.py --binary target/debug/kite-node --dsh-source /path/to/installed/deepseek-harness
```

The desktop fixture receives actual mouse, key/chord and UTF-8 events, including the maximum documented text size; screenshots verify PNG bytes, crop origin, private permissions, image blocks and conditional omission. Its process identity/focus is checked before input, it records only counters and known-text hashes, and it never uses real catalogue facts. Run it on a real desktop only through that desktop's launch environment and keep interaction confined to the owned window. The live-upgrade probe additionally checks managed-skill installation and preservation of user edits. Documentation tests cover missing-node access, multi-topic cache invalidation and fresh-context refetch; no global read marker is permitted.

Token budgets use pinned `tiktoken` reference encodings (`desktop/linux/qa/requirements.txt`). They report bytes, common first-load paths, unchanged responses and MCP schemas separately; they do not claim native Claude/DeepSeek token accounting or include image/reasoning costs. Consumer-loader tests use installed Codex/DeepSeek Harness metadata APIs without a model turn or harness config edits; missing optional installations are not evidence of live-model compatibility. Claude's common Agent Skills/MCP contract is separately documented.

The domain and foundation suites use in-process SSH / HTTP fixtures and do not require an external host:

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
./android/gradlew -p android :app:assembleDebug :app:lintDebug :app:testDebugUnitTest
```

Android JVM tests now cover the pure source-to-viewport transform, inverse touch mapping, letterbox rejection, pinch focus, pan limits and orientation preservation. CI runs `:app:testReleaseUnitTest` with Release assembly and lint. A future `NO-SOURCE` result must not be counted as a passing test suite.

Fullscreen pointer regressions are recorded in the [Alpha 14 acceptance](product/2026-10-31/virtual-pointer/acceptance.md). `DesktopPointerTest` checks the shared hotspot under zoom/pan, source and viewport edges, source-resolution changes and target clearing. `NativePresentationTest` draws the actual cursor view. `LiveVirtualMouseTest` is explicitly opt-in (`qa_live=true`, `qa_session=kite-qa-history-...`) and rejects a host other than `10.0.2.2:22223` / Sunshine `48200`; compare its exported source coordinates with the isolated host's button, release and wheel log. Refresh accessibility bounds after moving the control so the test cannot tap an obsolete button location.

The [Alpha 15 acceptance](product/2026-10-31/compact-mouse-and-hosts/acceptance.md) adds `ControlsPresentationTest` for real wheel tap/drag/cancel gestures, press state, feedback requests and host-card affordances. `WheelDetentsTest` checks accumulation, reversal, cancellation reset and rate limiting. The guarded live test also verifies upper/lower wheel taps and actual compact-control dimensions. Platform vibrator-manager history can confirm real system haptic dispatch; callback assertions and emulator motor emulation are not physical-phone tactile acceptance.

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


## Session activity and generated images

`SessionStatusTest` covers list labels and disconnected-state precedence. Rust session tests cover attention after completion, active requests, pause/start priority and failures. Host adapter tests cover native hosted/extension image records, large-result projection, byte-exact download, malformed/oversized images, cache repair and archived live sessions. The PR build also compiles debug Android instrumentation sources.

`SessionImagesPresentationTest` uses an owned synthetic PNG and a native overlay to check offline image-cache reuse, explicit status badges, preview and expansion/dismissal. It submits no model turn and needs no host. Its device execution is separate from CI compilation. A controlled real Codex generation and local history/image round trip are recorded in [session/image acceptance](product/2026-11-01/session-images/acceptance.md).


## Public commentary and reset cards

`ChatTimelineTest` and `ChatTimelineStateTest` now require non-tool assistant text to remain outside collapsed tool groups. The incremental case first shows commentary, appends a tool, updates history and resets the mirror; the text remains a Message row throughout. `CreditExpiryTest` covers countdown boundaries. Rust `usage` tests cover authoritative reset-card counts, complete/partial/absent expiration details, duplicate/redeemed rows, rolling updates, stale data and account switches.

`WorkspaceRefinementPresentationTest` adds native cases for visible public commentary beside collapsed tools, card counts/countdowns and unavailable expiry. These cases are compiled in PR CI; device execution is separate. [Finding and acceptance scope](product/2026-11-01/readable-progress-and-cards/acceptance.md).


## Pending questions and latest navigation

`ResponsePresentationTest` requires question/approval attention to override thinking/tool/responding presentation and restores the actual phase after resolution. Native presentation adds the explicit waiting label. `ChatTailMarginTest` uses 240/96 dp thresholds and replays tall historical user answers, negative scroll deltas, real tail/spacer bounds, added footer rows, growing replies, invalid layouts and transient cannot-scroll flags. Once visible, the button can hide only on a measured tail inside the buffer or an explicit reset. The guarded native drag uses the enlarged show threshold. [Finding and acceptance scope](product/2026-11-01/question-wait-and-tail/acceptance.md).
