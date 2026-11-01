# Visible Codex text and reset cards

## Client finding

The Alpha 31 Android client classified every Codex `phase=commentary` / `channel=commentary` message as `intermediate`. `chatEntries` grouped adjacent intermediate messages and tool calls into a Work row. A Work row starts collapsed, and only shows its messages after expansion. Its collapsed heading displays a call count and the last tool summary, without the assistant text.

A visible commentary could therefore disappear when the next tool arrived: the incremental timeline rewound over its trailing intermediate message, rebuilt the suffix as a Work row, and replaced the visible message with the collapsed group. Provider commentary includes public progress updates and interim conclusions. The client did not distinguish those texts by their wording.

Evidence in the released source:

- [Provider phase classification](https://github.com/wenshiqi0/Kite/blob/v0.2.0-alpha.31/crates/kite-agent/src/lib.rs#L328-L333).
- [Original group construction and incremental suffix rebuilding](https://github.com/wenshiqi0/Kite/blob/v0.2.0-alpha.31/android/app/src/main/java/io/github/wenshiqi0/kite/ui/ChatTimeline.kt).
- [Collapsed default and expanded-only message body](https://github.com/wenshiqi0/Kite/blob/v0.2.0-alpha.31/android/app/src/main/java/io/github/wenshiqi0/kite/ui/ToolGroupCard.kt).

The new projection groups adjacent tool calls only. Every non-tool assistant message remains a Message row, preserving its ID and native streaming presentation. User messages, interim conclusions, public commentary and final replies stay visible. Tool arguments/output retain their existing expandable group and per-call detail dialog. Private reasoning records remain outside this message parser.

## Reset-card presentation

The model menu's allowance preview and allowance dialog now show the server's available reset-card count, earliest expiry countdown and local expiry date. The countdown updates every 30 seconds while shown. The count is never derived from detail-row length or adjusted locally after expiry. Missing, partially returned, non-expiring and expired details have distinct labels. A partial detail set uses “known earliest expiry”, so it does not claim that unseen cards cannot expire sooner.

Read-only RPC against Codex 0.164.0 confirmed `rateLimitResetCredits.availableCount` and per-credit `expiresAt` in the live response. No model turn or credit redemption was requested. Private account counts, credit IDs and dates remain outside the repository. The [official OpenAI App Server protocol](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt) and the installed generated schema define the source fields and the distinction between omitted details and an empty detail list.

Rolling quota-window notifications retain reset-card facts. A racing full read can refresh cards while preserving newer window values. Account changes clear the prior account's cards; failed/stale reads retain their previous facts with stale presentation. Opaque credit IDs are used only transiently for duplicate detection and do not enter client snapshots.

## Validation

- 116 Rust workspace tests pass, including authoritative counts, nearest-expiry selection, capped/absent detail rows, duplicate/redeemed rows, refresh races and account changes.
- Rust formatting and Clippy with warnings denied pass; all 27 host adapter tests pass.
- `ChatTimelineTest` and `ChatTimelineStateTest` check that public text stays outside collapsed tool groups, including after a tool arrives, edits and a history reset. `CreditExpiryTest` covers day/hour/minute and expired boundaries.
- Native presentation cases check visible commentary with collapsed tool details, reset-card count/countdown and explicit missing expiry. Device execution remains pending; CI compilation is recorded separately from actual device rendering.

The [PR check](https://github.com/wenshiqi0/Kite/actions/runs/41299488862) compiles the Release app and debug instrumentation sources, runs Release lint and JVM tests, and checks the shared Rust code. Runtime rendering on an Android device is not established by this compilation.

## Published Alpha 32

[0.2.0-alpha.32](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.32), versionCode 34, source `f44bd41f8a67303e9a5b04ff66d58f625a85db0f`, is merged into `main` and published as a prerelease. [Main CI](https://github.com/wenshiqi0/Kite/actions/runs/41300023556) and [tag publication](https://github.com/wenshiqi0/Kite/actions/runs/41300033596) pass. The PR's client grouping/countdown JVM tests and native-source compilation also pass.

The public APK, host archive and manifest match `SHA256SUMS`. Independent checks confirm the stable certificate, v2/v3 block presence and 16 KiB native ZIP alignment; CI performs full v1/v2/v3 verification. The downloaded APK contains the new reset-credit record and expiry field. The host archive reports Alpha 32. [Exact public-package evidence](published-release.json).

Device rendering and phone installation were not repeated. The client behavior finding is confirmed by the released grouping source and the passing incremental-timeline regression; this is not a claim to have reproduced the user's specific conversation on their handset.
