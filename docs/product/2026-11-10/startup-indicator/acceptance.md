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

The isolated regression reproduces the client state sequence; it is not a physical-handset execution. No Android device is connected to the local ADB server. Publication and tagged CI evidence are recorded after the build completes.
