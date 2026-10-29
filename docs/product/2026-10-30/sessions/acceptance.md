# Android Alpha 7 sessions and interactions — 2026-10-30

Controlled validation used an Android 16 / API 36 ARM64 emulator, a Linux X11 host, herdr 0.9.5 and Codex 0.164.0. Screenshots below show disposable validation conversations. No test prompts were sent to the user's production conversation, and no real Claude connection was started.

## Tool activity

Four separate real Codex `exec` calls were rendered as one [default collapsed group](tools-collapsed.png). The [expanded group](tools-expanded.png) exposes chronological calls and public progress notes within a 320 dp maximum panel; each call retains its [arguments and output](tool-detail.png). Actual assistant replies and user messages stay outside the group. Offline tests also project eighty calls with interleaved progress into one row and verify that appends/completions keep the group's identity. Inspecting logs pauses automatic tail following.

## Answering the model

A real Codex request asked two questions. The phone selected 蓝色 for the first and entered 来自手机的自由回答 for the second. The model returned `KITE-ANDROID-FORM` with both exact answers. Questions have [choices](question-choice.png), [free-form input](question-text.png) when offered, multi-question navigation and explicit submission. Their form is distinct from execution approval.

The APK was replaced while the request was still waiting. The new client received the pending question again and answered it successfully. A separate Rust probe also reconnected between receiving and answering a two-question form, then verified the actual provider reply and idle completion. Request/turn/thread correlation, stale responses, restricted approvals and nonblocking question lifetime are covered by offline tests. Submitted question answers are normalized from provider history into user reply bubbles.

## Durable sessions and provider choice

The phone created a real Codex session in a custom host project directory, used `pwd` to verify that directory and received `KITE-NEW-SESSION-OK`. It archived that conversation, reopened its records, restored it and received `KITE-SESSION-CONTINUED` on the same provider thread.

A separate live probe archived a session while its `sleep 3` task ran, observed the final reply afterward, closed the disposable terminal and restored the original provider thread. Two concurrent restore requests returned one terminal and one owner. Provider history remained in place. The phone also restored the Codex record after process restart, displayed its cached messages after disconnecting and reconnected to the same saved session.

[Archived records](archived-list.png) are separate from active sessions. A Claude session was saved, archived, restored and retained across an app process restart, while its reference and terminal binding remained empty. The [pending Claude view](claude-pending.png) explicitly says it is not connected. The connection adapter has a separate explicit entry; live Claude is outside this validation scope.

## Visible response state

The app immediately shows sending feedback and then [thinking](thinking.png), tool or reply activity. Waiting for the user, completion, failure and disconnection stop thinking animation. During a real model request and shell task, native UI checks observed the changing labels. A video recorded the indicator; dot-region pixels changed between frames 250 ms apart. The emulator's original motion setting was 0; validation temporarily enabled 1 for this recording and restores the original setting afterward. With system motion disabled, text feedback remains visible.

## Validation and delivery

Rust workspace tests and Clippy, eleven Android JVM tests, Android lint, four host-adapter tests, three publisher tests and an iOS ARM64 simulator build passed. The trusted tagged CI must build, sign and verify the Release APK before [publishing Alpha 7](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.7). Final delivery uses that GitHub Release attachment, preserving the existing signing certificate, device identity, host trust and pairing.

The host adapter is versioned as `backend-v2.py`, with a revisioned private session catalog. [RFC 0007](../../../rfcs/0007-sessions-and-interactions.md) records the boundaries and [host setup](../../../backend-setup.md) describes usage. Codex app-server remains experimental. Claude is deliberately a pending connection in this validation; there are no fabricated Claude replies.
