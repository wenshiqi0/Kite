# Native paginated history and responsive conversation selection

The Alpha 96 host gate fix did not cover all of the user's failure. Native Codex
paginated conversations store public items in `thread_history_1.sqlite` without a
JSONL rollout. The old client stopped at the missing file. Conversation browsing
also awaited a remote catalog and full source/composer refresh under selection
operation locks, so slow reads prevented further session changes.

Alpha 98 adds a read-only native-store adapter. Public native UI items are
projected to the existing history protocol in a private, append-only Kite cache.
Provider databases, thread identities and model context remain unchanged.
Reasoning items are excluded, source ordinals and terminal errors retain their
ordering, unchanged reads retain their stamp, and native rollback only resets
the derived cache. The optional host capability is `historyNativeApi=1`.

Conversation selection now uses the existing local catalog immediately. An
independent session-scoped task reads history without waiting for model settings,
quota or terminal reads. A cache-free client shows its latest ordered page while
older pages backfill; cached history continues to remain readable. Data fetches
cannot take over another selected conversation.

Validation includes native SQLite fixtures (public/private filtering, source
immutability, append/update, unchanged stamp and rollback), the actual Rust
reader over authenticated SSH and the Node/Bun bridge with no rollout file,
and eight sequential selections while the remote catalog deliberately stalls.
An opt-in native Android test connects two pinned owned SSH peers backed by real
Nodes and synthetic native databases, loads large multi-page histories, switches
between four sessions and measures selection/UI-pulse latency. No model turn is
sent and no real user host authorization is modified by the fixture.

Validation on 2026-11-11:

- Bun type/lint/boundary checks and 115 tests passed; one Darwin-only case skipped.
- 232 Rust tests passed with the host feature, including the native-database
  authenticated SSH/Node/Bun path and blocked-catalog selection case. Strict
  clippy passed, including the owned host example.
- 85 Android Release JVM tests, Release lint and native source compilation passed.
- `NativeHistoryNavigationTest` passed against two pinned, loopback-only SSH
  peers running the real Node and synthetic native SQLite stores. Four sessions
  each contain 450 tool calls and a substantive final answer, spanning multiple
  history pages. Eight selections took 21–83 ms, UI pulses took 0–1 ms and each
  target's final answer was verified in the actual accessibility tree. The latest
  page appeared before complete older-page backfill. No model turn was sent.
- The real local native database was read without modification for three
  previously unresolvable references; 79, 190 and five public message rows were
  available, and their projections were produced in 18–101 ms.

The emulator run validates the full native client path, not a manually injected
message mirror. Its owned fixture did not use or alter real SSH authorization.
Physical-handset results remain separate. Public delivery is recorded below.

Alpha 97 was not published: both tag attempts failed while installing Ubuntu
QA dependencies because the hosted image could not resolve its Azure mirror.
Alpha 98 retains the same functional implementation and validation, changes
only version metadata and the workflow to use signed official Ubuntu sources
with bounded acquisition timeouts/retries, and keeps all checks enabled.
