# Session deletion acceptance — 2026-11-03

Active and archived Android session lists and the current conversation menu offer Delete session. Confirmation names the target and authorizes immediate termination. Rust binds the command to its host, clears the conversation cache and selects another active record or an empty workspace. The host stops verified owners and their tools before removing the record; failed release retains the record. Identity markers fence stale catalog writes and discovery, including a Codex thread whose initial binding has not finished. Original provider history stays in place.

## Validation on wen.linux

- `bun run check`: strict typecheck, Biome and package boundary checks; **59 tests passed**.
- Owned-process acceptance uses the built host CLI, a private Unix WebSocket app-server peer and a tool that ignores SIGTERM. Bound and unbound owners and their tools exit before successful deletion. A shared backend and an unrelated foreground process remain alive and their records remain visible.
- Provider regressions cover failed-release retry, drafts, duplicate deletion, shared references/panes/aliases, owner/catalog leases, changed identity and stale rediscovery.
- `cargo test -p kite-store -p kite-agent -p kite-app -p kite-ffi --features kite-app/host-node --locked`: **141 tests passed**, including selected/background/last-session deletion and stale cache/catalog protection. The final application changes also passed all **76 kite-app tests**.
- Rust formatting and Clippy with `-D warnings` passed for changed crates and their test targets.
- Android `compileReleaseKotlin`, `compileDebugAndroidTestKotlin`, `testReleaseUnitTest` and `lintRelease` passed using JDK 21, SDK 37 and NDK 30; **75 JVM tests passed**. The library and generated bindings came from the same build.

## Acceptance boundaries

The native delete-dialog cancellation case was compiled, not executed on a device. Actual systemd/cgroup termination of DSH, Ply and Claude was not repeated against a live host. The controlled real-process checks cover the Codex process-group path. No user sessions were deleted, no production host was restarted or upgraded, and no APK was packaged or published in this source change. Both the client and host need this operation before delivery to a device.
