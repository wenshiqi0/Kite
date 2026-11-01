# Android session activity and Codex images

Session rows show running, waiting for an answer, attention/approval, starting, paused, completed, idle, failed, saved and disconnected states. The Rust core reconciles one host batch of control facts for all live sessions, including archived sessions and sessions not opened by this phone. Unanswered asynchronous questions remain visible after a turn completes. Native labels and coloured dots do not replace the underlying provider state.

Generated images are assistant attachment messages with immutable SHA-256 identities. The host projects hosted `image_generation_call`, legacy `image_generation_end`, and hosted/extension `item_completed` records into the same form. Large base64 results stay out of conversation snapshots and deltas. Android downloads through the authenticated SSH connection, verifies content, retains local files for offline viewing, samples preview bitmaps to 2048 pixels, and offers zoom and system Documents export. PNG, JPEG and WebP up to 25 MiB are accepted; unavailable/invalid/oversized images do not erase surrounding chat.

## Evidence

- `cargo test --workspace --locked`: 110 tests passed.
- An isolated real Node socket test transports a 6 MiB binary result through the same framed bridge used by SSH; route tests require both new commands to be allowed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo fmt --all --check`: passed.
- `python3 -m unittest discover -s backend -p 'test_*.py'`: 27 tests passed, including a 5 MiB result projected below 2 KiB, byte-exact download, malformed data, source pagination, file-backed completion, archived sessions, oversize isolation, corruption rejection and cache repair.
- A controlled Codex CLI 0.164.0 turn generated one blue-square PNG through the real image-generation tool. Its actual `Extension` / `image_gen.generation` rollout was projected by the new host adapter and parsed by `kite-agent` into five messages with one image attachment. The downloaded image was visually inspected and matched its original SHA-256. Exact dimensions, sizes and identity are in [the round-trip record](image-roundtrip.json). The owned prompt invoked no shell command. This is a real generation and local projection/download check, not an Android device rendering check.
- `SessionStatusTest` covers native label selection and disconnected-state precedence. `SessionImagesPresentationTest` uses an owned synthetic PNG to check offline caching, actual native status badges, preview, expansion and dismissal. CI compiles native presentation tests; device execution remains pending.

The Android app and Linux host must both be updated to the matching release for the new host projection and image-download commands. The change uses the existing Codex host configuration and account. Protocol references: [official OpenAI App Server documentation](https://learn.chatgpt.com/docs/app-server), verified against the installed 0.164.0 generated JSON schema and upstream OpenAI protocol/extension source.
