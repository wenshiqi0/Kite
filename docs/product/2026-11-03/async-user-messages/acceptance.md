# Codex asynchronous user-message presentation

The reported Android screenshot displayed a `send_message_to_user_async` call and its `accepted` receipt as a generic tool. The original function's `message` argument is intended for the user.

`kite-agent::Message::presentation` now projects a valid call into an assistant message with the same call ID, full text and no tool field. `kite-ffi::message_info` uses this projection for native conversation deltas. Raw records and serialized caches retain the original tool and receipt, so existing offline caches receive the fix immediately and paging/merge semantics remain unchanged. Ordinary tools, asynchronous question forms, malformed messages and explicitly failed calls retain their existing details.

Local validation:

- `cargo test -p kite-agent -p kite-ffi --locked`: 70 tests pass (64 agent unit, 4 authenticated SSH fixture, 2 FFI tests).
- Dedicated regressions cover message delivery before/after acknowledgement, Markdown and newlines, repeated records, serialization/restoration, both page arrival orders, receipt merging, malformed inputs, failed delivery, unrelated tools and the actual FFI message mapping.
- `cargo clippy -p kite-agent -p kite-ffi --all-targets --locked -- -D warnings`, `cargo fmt --all --check` and `git diff --check` pass.

The new worktree's pinned native submodules and Bun dependencies were initialized before running the successful checks. No user conversation was resumed or sent a message. This change does not require a Node upgrade. Android installation and physical-phone presentation are not claimed by these local tests; the tagged Alpha workflow supplies APK compilation, JVM tests, lint and publication checks.
