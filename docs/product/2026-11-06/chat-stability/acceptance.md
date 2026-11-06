# Chat stability across transport and layout changes

This change builds on [PR #20](https://github.com/wenshiqi0/Kite/pull/20), Alpha 87 (`a3fcffd5c6e5b823158b79f4705bb0e8fbcec645`). The earlier fix handled an explicit task-panel toggle. The same underlying problem could still be triggered by connection notices and other changes to the chat viewport.

Three layers amplified each other:

- Android rendered two reconnect notices, including variable-length errors cleared at the start of every retry. Native draw sampling reproduced **189px** temporary message displacement as these errors appeared and disappeared.
- Rust transport teardown drained saved conversation runtimes and then restored their caches. It could publish an empty read model and advance the message revision despite no message change. The regression failed on the baseline revision changing from 2 to 3 after teardown.
- A same-context full transcript reset discarded native rows and tool-group identities. A backfilled tool call could therefore collapse an expanded group. The baseline row-identity regression failed.

The transport now cancels jobs and expires input authority while retaining bounded saved-conversation read models, journals and revisions. Ephemeral panes are retired; client destruction retains its full cleanup path. Final readable histories are persisted after transport jobs stop. A recording observer regression checks **every publication** across four teardowns and a rebind, including unchanged message identity/revision, expired approvals/input, cancelled synchronization and discarded ephemeral state. Existing local SSH recovery tests reject work-starting commands and verify recovery without replay.

Android uses one fixed-height connection notice; its Details action opens the existing diagnostics dialog. `ChatViewport` applies the tail request during measurement whenever available width or height changes. This replaces the task-specific toggle callback and covers panels, connection chrome and composer/inset-driven resizing through the same path. A history reader retains the normal lazy-list anchor. Scroll state is scoped to the actual conversation; the resize measurement does not subscribe itself to scroll-layout feedback. Same-context transcript resets reconcile existing rows and preserve overlapping tool-group keys.

Native execution uses an isolated Android 16/API 36 x86_64 emulator and synthetic display snapshots, with matching Rust/UniFFI per build. **12 cases pass**: repeated reconnect errors and readable diagnostics, viewport resize/history anchoring, all nine related-task regressions, and streaming Markdown retention/growth/completion. The two new cases also run against the final Alpha 88 debug build. [Measured draws](native-validation.json) record no retry displacement and a constant measured tail gap through five viewport sizes; history remains at item 5 / offset 11. Connection snapshots are injected into the native display mirror; they are not a physical-phone network-outage test.

**92 Rust tests**, **84 Release JVM tests**, Release lint, strict Rust Clippy and **9 publisher tests** pass. Physical-handset acceptance and a real user-host outage remain separate from the isolated UI checks and local SSH fixtures.

Delivery uses [Alpha 88](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.88), Android versionCode **90**, via `android/scripts/release-alpha.py`. The host protocol is unchanged; this repair is delivered by updating the Android client.
