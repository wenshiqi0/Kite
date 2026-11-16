# Android initialization indicator

The user identified the continuously moving thin bar below the chat title. `KiteWorkspace` displays this indicator while the core snapshot is absent or `initialized` is false.

## Reproduction

During `AppCore::initialize`, `sync_store` publishes the saved host before the device key and initialization completion are published. Android requests a conversation delta for each snapshot. That read calls `ClientCore::selected` and can create a workspace from the incomplete base snapshot. The workspace retains `initialized = false` and no device key even after base initialization completes. The selected workspace then keeps the Android indicator active.

The regression `core::client::tests::reading_history_during_initialization_keeps_workspace_ready` requests that delta at the intermediate publication. Before the fix it fails at `assert!(snapshot.initialized)`; after the fix it passes and also checks that the device key and host selection are retained.

## Fix and validation

`ClientCore::runtime` rejects workspace creation until the base is initialized. The existing `conversation_since` fallback serves the base while initialization is incomplete. After initialization, workspaces inherit the complete state normally.

- The focused regression passes after reproducing the prior failure.
- `cargo test -p kite-app --lib --locked`: 93 passed.
- `cargo fmt --all --check`: passed.
- `cargo clippy -p kite-app --all-targets --locked -- -D warnings`: passed.
- Alpha 89 is based on the published Alpha 88 source, retaining its previous fixes.

The isolated regression reproduces the client state sequence; it is not a physical-handset execution. No Android device is connected to the local ADB server.

## Published delivery

[Alpha 89](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.89), versionCode 91, is published from source `e2e5c7317e04ec18a1096764e25d3dd2c3b86450`. Both the build and publication jobs in [tagged CI](https://github.com/wenshiqi0/Kite/actions/runs/42735025504) pass, including full Rust checks, Linux native checks, Release Android lint/JVM tests and signing/package checks.

Fresh downloads of both public APKs match the public checksums and manifest. SDK verification independently confirms the Alpha 88 certificate, Release variant/version, arm64 ABI, API-23 v1 signatures, API-36 primary v3 and compatibility v2 signatures, identical application payloads and 16 KiB alignment. See [published package evidence](published-release.json).

Update the Android client; the host protocol is unchanged. Physical-handset startup acceptance remains separate.
