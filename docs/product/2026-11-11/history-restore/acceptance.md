# Restore history after a clean phone installation

Root cause: Alpha 94/95 added `--provider` to the history request, but the Rust
Node bridge did not allow that option. The request failed before the TypeScript
history adapter ran. Existing phone caches could hide the failed reads; clearing
app storage made saved sessions appear with empty message bodies.

The fix keeps the established Codex history command for compatible older hosts.
The Node gate also accepts the already-released clients' provider selector,
restricted to `history` and the known `codex` / `claude` values. The host advertises
`historyProjectionApi=1`; Claude only uses provider-aware projection when that
capability is present and otherwise retains the original JSONL reader. Failed
reads retain a bounded diagnostic cause.

Validation:

- Real authenticated SSH tests cross the generated shell, the real Node gate,
  binary frames and the bundled TypeScript adapter. A fresh client with no cache
  retrieves existing substantive replies and appended new replies on both
  original strict hosts and current hosts. Claude tests cover projected and
  original JSONL history.
- Unknown providers, missing values and provider options on unrelated backend
  actions remain rejected.
- 230 targeted Rust tests passed with `kite-app/host-node` enabled; strict clippy
  and formatting passed. Alpha 95's bubble styling is unchanged.
- The live host reproduces the original `backend option denied` failure using an
  owned synthetic history file. The established command reads the same fixture.
  The live catalog contains 111 saved references, and the current conversation's
  provider history file exists and is nonempty. No model turn was sent.

Publication and compatible live host upgrade verification are recorded below.
