# Native paginated history and responsive conversation selection

The Alpha 96 host gate fix did not cover all of the user's failure. Native Codex
paginated conversations store public items in `thread_history_1.sqlite` without a
JSONL rollout. The old client stopped at the missing file. Conversation browsing
also awaited a remote catalog and full source/composer refresh under selection
operation locks, so slow reads prevented further session changes.

Alpha 97 adds a read-only native-store adapter. Public native UI items are
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

Execution and public-delivery results are recorded after verification.
