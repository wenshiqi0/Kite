# Codex subagent trees

The host discovers spawned descendants through the existing Codex owner using metadata-only `thread/list` calls, including archived descendants. Direct parent thread IDs define the tree. The existing parent/pane reconciliation remains in place, and discovery does not start or resume a model turn.

Rust maps native runtime flags and bounded persisted outcome facts into shared states. Active runtime facts override older completion records. Saved conversations retain recorded child trees, while disconnected or unavailable runtime facts cannot retain a running label. Native clients render the tree and branch expansion locally.

Verified before publication:

- 91 Bun tests pass, one Darwin-only case is skipped; TypeScript, lint and dependency boundaries pass.
- 184 Rust tests pass across the affected crates and integration suites; one opt-in historical-fixture test is ignored. Formatting and strict Clippy pass.
- 84 Android JVM tests pass; Release lint and native test-source compilation pass.
- A read-only scan through the real installed Herdr and existing Codex owner returns the current worker under its original parent with no subagent scan error. Independent metadata reads also recover a real parent/child/grandchild tree with completed and interrupted outcomes. Conversation bodies are not copied into this repository.

Android native component execution was not completed: the x86_64 NDK link encountered the existing Moonlight CPU-dispatch relocation issue. No fixture linker change was shipped. Android handset rendering and macOS device rendering remain unverified; source compilation and JVM tests are separate evidence.

## Published package and installed host

- [Alpha 83](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.83), Android versionCode 85, is published as a prerelease from source `cd80001448a271d8100db6e1d3ff525d92dd9f95`.
- [Tagged CI](https://github.com/wenshiqi0/Kite/actions/runs/41849542263) passes on attempt 2. The first attempt stopped at the unchanged native Linux panel screenshot probe; retry passed without source changes.
- All four public asset checksums match the downloaded files. Primary and compatibility APKs retain the Alpha 82 certificate, pass independent API-23 and API-36 signature checks and 16 KiB alignment, and contain identical application payloads. Package ID and version match the release.
- The public Linux host is installed by the matching installer's compatible handover. Its running binary matches the published package; the node identity, effective configuration, external services, all 78 saved references and all three verified active Codex owners remain intact.
- A read-only `agent.list` through the installed Alpha 83 CLI returns the actual worker under its existing parent, with one child tree and no scan errors. No model turn is sent. The Android handset installation remains a user-side update.
