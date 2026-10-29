# Testing

## Offline checks

The domain and foundation suites use in-process SSH / HTTP fixtures and do not require an external host:

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
./android/gradlew -p android :app:assembleDebug :app:lintDebug :app:testDebugUnitTest
```

There are currently no Android JVM unit test sources. A `NO-SOURCE` result from `testDebugUnitTest` is not a passing unit test suite. CI builds and lints the Release variant separately.

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

The most recent development acceptance used an ARM64 Android API 36 emulator, a Linux X11 desktop, herdr 0.9.5 and Sunshine v2026.914.233613 over LAN. A 1920×1080 source was decoded at 1920×1080, with a 60fps request; the host refreshed at 50Hz and a SurfaceFlinger sample showed approximately 50 presented frames per second. These are emulator observations, not physical-phone or WAN acceptance. Source changes, build checks, host state and user-perceived latency are separate evidence.

## Optional integration tools

| Tool | Scope |
| --- | --- |
| `crates/kite-app/examples/desktop_probe.rs` | Real SSH, herdr API, pairing and encoded transport; no decoder assertion |
| `crates/kite-app/examples/desktop_fixture.py` | Linux GTK3 animated target; records input counters and one exact test-text match |
| `android/ui-probe` | Independent test-only accessibility package for entering Unicode in the app's text composer |

The host fixture is not a product runtime dependency. It restores the prior focus on exit and does not change the display mode. Input testing must target that live fixture. Remove temporary SSH authorizations and test helper apps afterward; keep the user's desktop session and normal Sunshine service.

A screenshot-based click round trip includes input-dispatch and screenshot overhead. Do not report it as a pure touch-to-photon latency measurement. Likewise, a requested 60fps mode is not proof of sustained 60fps presentation.
