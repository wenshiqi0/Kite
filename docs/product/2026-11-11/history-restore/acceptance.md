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

Published [Alpha 96](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.96)
/ versionCode 98 from `cedfd13a425bbd828b60a556e7e26cd8cb019c25`.
Both [tagged CI jobs](https://github.com/wenshiqi0/Kite/actions/runs/43014500645)
passed. Independent public downloads passed checksum, stable-certificate,
signature, application-payload and 16 KiB alignment checks.

Live recovery verification:

- wen-h255 and wen-mini run the exact public Alpha 96 Node binary and advertise
  history projection support. Both hosts are ready.
- Node identities, all 111 / seven saved session references, and the running
  Herdr/Sunshine process IDs were preserved.
- The original Alpha 94/95 phone history request now succeeds on both hosts.
- A cache-free read of the current conversation on wen-h255 traversed all three
  history pages and retrieved 44 message rows, including two substantive
  assistant answers. No model turn or user-message replay was performed.

Existing Alpha 94/95 clients can retry synchronization after the host fix;
another app uninstall is unnecessary. Alpha 96 also makes the Codex client
request compatible with hosts that still use the original history interface.
