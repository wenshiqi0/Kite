# Codex subagent trees

The host discovers spawned descendants through the existing Codex owner using metadata-only `thread/list` calls, including archived descendants. Direct parent thread IDs define the tree. The existing parent/pane reconciliation remains in place, and discovery does not start or resume a model turn.

Rust maps native runtime flags and bounded persisted outcome facts into shared states. Active runtime facts override older completion records. Saved conversations retain recorded child trees, while disconnected or unavailable runtime facts cannot retain a running label. Native clients render the tree and branch expansion locally.

Verified before publication:

- 91 Bun tests pass, one Darwin-only case is skipped; TypeScript, lint and dependency boundaries pass.
- 184 Rust tests pass across the affected crates and integration suites; one opt-in historical-fixture test is ignored. Formatting and strict Clippy pass.
- 84 Android JVM tests pass; Release lint and native test-source compilation pass.
- A read-only scan through the real installed Herdr and existing Codex owner returns the current worker under its original parent with no subagent scan error. Independent metadata reads also recover a real parent/child/grandchild tree with completed and interrupted outcomes. Conversation bodies are not copied into this repository.

Android native rendering is checked separately with a synthetic three-node tree and a temporary application ID. The normal x86_64 NDK link encounters a pre-existing Moonlight CPU-dispatch relocation issue; any fixture-only linker workaround must be recorded and must not be counted as validation of the published ARM64 binary. macOS device rendering is not verified on this Linux host. Public package verification and installation evidence are recorded after the tagged build completes.
