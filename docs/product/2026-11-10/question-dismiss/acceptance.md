# Close model questions and continue chatting

Pending-question hints shared the same `composeReason` value as actual send restrictions, so Android disabled ordinary messages even for asynchronous questions. Blocking questions additionally hid the composer, and both Core and the chat client rejected new messages. Answer cards offered navigation and submission without an explicit close action.

Android now offers **稍后回答** throughout the form. The inline card collapses and retains its current page and field values; the answer sheet closes and retains field values. **回答问题** reopens the form. Closing only changes presentation. Both question types keep the normal composer available; connection, initialization, pause, upload and submission restrictions continue to apply.

When a user explicitly sends a new message, Rust validates its input and settings first, then resolves pending blocking question RPCs with empty `answers` before sending the message. No offered answer is selected or fabricated. Only the original question request IDs are used; approval requests are untouched. Failed response writes retain the pending form and do not send the message. Ordinary messages supersede asynchronous forms through the existing history-input boundary.

Validation on 2026-11-10:

- **185 Rust tests** pass across agent, app and FFI, including two new regressions for multiple blocking requests, response identity, preparing without discarding forms, unrelated approvals, nonblocking questions, expiry and other threads. One opt-in real-report test is ignored. Formatting and strict all-target Clippy pass.
- **85 Release JVM tests**, Release lint, instrumentation compilation and the matching x86_64 Rust/UniFFI debug build pass.
- The new native test `WorkspaceRefinementPresentationTest.questionsCanCloseMidFormKeepDraftsAndLeaveTheWorkspaceComposerEnabled` passes on an isolated, network-disabled Android API 36 emulator with an empty app. It exercises the actual workspace for **both blocking and nonblocking forms**: choose the first answer, move to the second page, type a draft, close, verify the enabled composer, reopen the same inline page/draft, open and explicitly close the answer sheet, and verify that no answer was submitted. [Collapsed question and enabled composer](closed-question.png), [reopened draft](reopened-draft.png).
- A separate **real Codex 0.166.0 ephemeral app-server thread** asks a blocking question, accepts an empty-answer RPC immediately followed by a normal user turn, and completes with the requested marker. [Provider result](provider-validation.json). This verifies the native protocol rather than a full Kite/SSH/host round trip.

The first native attempt was blocked by an Android System UI ANR dialog during fresh emulator startup. After dismissing that system dialog, the final APK passed. Physical-handset installation and rendering remain untested.

Version **0.2.0-alpha.92**, Android versionCode **94**, is based on published Alpha 91. The host protocol is unchanged; only the Android client needs updating.
