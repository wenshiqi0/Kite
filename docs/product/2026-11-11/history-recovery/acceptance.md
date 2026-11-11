# History and substantive-reply recovery after a fresh install

Alpha 94/95 added `--provider` to all native history requests without allowing
that option in the Rust Node bridge. Both old and matching published host
gateways rejected the request before the TypeScript history reader ran. The
runtime/composer could still publish thinking/tool activity, while the separately
polled message body failed to load. Existing phone caches could mask the missing
history; clearing app data removed that fallback.

On both user-connected Linux hosts, the installed Node was Alpha 84. The original
`history --file ...` read succeeded on an owned transcript; adding `--provider`
failed with `node protocol: backend option denied`. Saved host catalogs remained
readable (111 records on wen-h255 and seven on wen-mini).

Alpha 96 fixes both sides of the contract:

- Codex uses the established history request without the unnecessary flag, so
  fresh clients can still read existing compatible hosts.
- The Node gateway permits `--provider codex|claude` only for the history reader.
  Existing Alpha 94/95 clients regain message loading after the host update.
- Hosts advertise `historyProjectionApi=1`. Claude uses projected history only
  with that capability; older hosts retain their original bounded JSONL read.
- Bounded remote failure reasons are retained for diagnosis instead of losing
  the bridge's rejection cause.

Regression coverage uses the actual Rust client history reader, authenticated
SSH, a real private Node socket/allowlist, binary framing and the bundled Bun
adapter. Only the transport-only CLI peer is a fixture; projection and parsing
are production code. Cases cover no client cache, tool activity followed by a
substantive final answer, unchanged-cache reuse, a newly appended final answer,
a second fresh read and both old/new Claude capabilities. No model turn is sent.

Local checks passed: 230 Rust tests with `kite-app/host-node` enabled, strict
clippy, 85 Android Release JVM tests, Release lint and native test compilation.
UI styling and the shared multimodal renderer are unchanged from Alpha 95.

Publication and the two real-host recovery checks are recorded below.
