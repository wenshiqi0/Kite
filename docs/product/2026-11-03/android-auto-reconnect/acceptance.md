# Android connection recovery — 2026-11-03

The previous client stopped at Failed after SSH loss, stopped its foreground service, and opened settings. The new Rust recovery task retains explicit connection intent and retries transient transport failures with capped 1/2/4/8/16/30 second delays. Offline recovery waits for a network event. Native default-network callbacks and return-to-foreground checks wake read-only health probes. Disconnect clears persisted intent and cancels the retry; host switches and active configuration edits cancel the old generation.

Android keeps the foreground service during recovery and requests START_STICKY for eligible service recreation. Application initialization restores the Rust-owned connection intent. The service still promotes immediately before handling asynchronous initialization/failure. Recovery displays status and a disconnect action, retains the conversation view, and avoids automatically opening settings. Video can recover in view-only mode; input takeover is released on loss.

Automatic transport restoration observes existing owners and catalogs. It never creates or resumes an owner, starts a model turn, resends a prompt or approval, or replays desktop input. Authentication rejection and host-key errors require user action. Missing/corrupt intent does not bypass trust; corrupt bytes are preserved and chat can still initialize.

## Validation on wen.linux

- **80 kite-app tests** and kite-ffi test/doc-test targets passed with `host-node` enabled.
- Local authenticated SSH acceptance closed a real client transport, then observed automatic reconnection to the same saved session and complete cached conversation. The fixture rejects every session/owner creation and model-turn command. A recreated Core restores the saved connection and conversation.
- Local SSH tests cover offline waiting, immediate network-return recovery, explicit disconnect and process recreation without reconnect, host-switch cancellation, authentication refusal without retry, and corrupt-intent preservation.
- Delay caps and strict authentication/trust exclusions have deterministic domain tests.
- Android **77 JVM tests** passed, including Wi-Fi/cellular/VPN callback ordering and foreground refresh on an unchanged network.
- Release Kotlin, debug instrumentation source compilation, Release lint, same-build Rust/UniFFI generation, Rust formatting and Clippy with `-D warnings` passed.
- `bun run check` passed strict TypeScript, package boundaries and **47 existing server tests**.

## Acceptance boundaries

Real Android process reclamation/START_STICKY restart, screen-off Doze, OEM battery policy, real Wi-Fi-to-cellular handover, and video recovery remain device acceptance. Native code was compiled and JVM facts were tested; no phone or emulator UI execution is claimed here. No APK was packaged/published and no production host/service was upgraded for this source change.
