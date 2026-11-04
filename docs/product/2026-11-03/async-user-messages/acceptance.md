# Codex asynchronous user-message presentation

The reported Android screenshot displayed a `send_message_to_user_async` call and its `accepted` receipt as a generic tool. The original function's `message` argument is intended for the user.

`kite-agent::Message::presentation` now projects a valid call into an assistant message with the same call ID, full text and no tool field. `kite-ffi::message_info` uses this projection for native conversation deltas. Raw records and serialized caches retain the original tool and receipt, so existing offline caches receive the fix immediately and paging/merge semantics remain unchanged. Ordinary tools, asynchronous question forms, malformed messages and explicitly failed calls retain their existing details.

Local validation:

- `cargo test -p kite-agent -p kite-ffi --locked`: 70 tests pass (64 agent unit, 4 authenticated SSH fixture, 2 FFI tests).
- Dedicated regressions cover message delivery before/after acknowledgement, Markdown and newlines, repeated records, serialization/restoration, both page arrival orders, receipt merging, malformed inputs, failed delivery, unrelated tools and the actual FFI message mapping.
- `cargo clippy -p kite-agent -p kite-ffi --all-targets --locked -- -D warnings`, `cargo fmt --all --check` and `git diff --check` pass.

The new worktree's pinned native submodules and Bun dependencies were initialized before running the successful checks. No user conversation was resumed or sent a message. This change does not require a Node upgrade. Android installation and physical-phone presentation are not claimed by these local tests; the tagged Alpha workflow supplies APK compilation, JVM tests, lint and publication checks.

## Published Alpha 73

[Alpha 73](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.73), versionCode 75, is published from `c3240db3fd20bb27f32aa2b39196a0b5d931d890`. Both jobs in the [tagged workflow](https://github.com/wenshiqi0/Kite/actions/runs/41653012627) passed, including complete Rust checks, native Linux fixtures, Android JVM tests/lint, signing/alignment verification and the publisher's full public-asset downloads/checksums.

A separate read of the published manifest and checksum list confirms the source commit, version, stable certificate pin and both APK digests against GitHub's public asset metadata. Full APK signature and content verification was performed by CI. Exact publication facts are in [publication.json](publication.json). No handset installation or physical UI acceptance is claimed.
