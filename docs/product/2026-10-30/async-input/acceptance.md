# Android Alpha 9 async questions — 2026-10-30

Validation uses an Android 16 / API 36 ARM64 emulator, herdr 0.9.5 and Codex 0.164.0. This establishes emulator behavior, not physical-phone acceptance. Test prompts and answers use a disposable validation session. The user's existing question is inspected read-only and is left for the user to answer.

## Cause and behavior

The actual missing request was `request_user_input_async`. It emits an `agentMessage` with `delivery: "async"` and structured `questions`; it does not issue the blocking `item/tool/requestUserInput` RPC that Alpha 8 handled. The old client therefore showed a generic terminal-input hint or a completed tool group without an answer form.

Questions now appear in chat as [choices](choices.png) with optional free text. A persistent **回答问题 / Answer questions** action near the composer opens the [same form in a sheet](answer-sheet.png), so split view and a scrolled transcript do not hide the controls. Nothing is selected or submitted automatically. Async questions keep ordinary chat enabled. Native blocking questions retain their separate RPC reply and take priority over nonblocking forms.

## Real provider checks

The model asked two questions, then finished its turn before the new APK was installed. After replacement and reconnection, the phone recovered both fields. It selected `绿色` and entered `来自手机的异步回答`. Both values survived closing and reopening the answer sheet. The phone submitted them and the actual model returned `KITE-ASYNC-ANSWERED 绿色 来自手机的异步回答`. Chat history shows readable answers without transport tags, and the resolved form disappears.

A separate request asked for `只看` or `操作`, then kept its turn running with an interruptible sleep. While desktop split was open, the phone used the answer sheet to select `操作`. The correlated user answer and `KITE-ACTIVE-ANSWERED 操作` response belonged to the original turn ID; no second turn was started. Submission uses `turn/steer` with the active ID; a completed turn instead uses `turn/start` on the same thread.

## Older questions and bounded history

The user's actual question had already fallen outside the recent 2 MiB display history. A full-log control index now retains original async calls, results and user-input boundaries independently of that display window. The host tracks file identity and complete-line offsets, removes inline image data and stores only private, rebuildable source facts. Rust determines which questions remain unanswered. Ordinary message bodies are not copied into the index; only question answers retain their text. Provider history is never rewritten.

Backend tests put a question before 5,000 unrelated records and verify that it remains in the control segment after disappearing from recent chat. They also cover incremental answer capture, source replacement and truncation, incomplete lines, image preservation in ordinary history, bounded transport and private cache permissions. Rust tests cover framed decoding, exact field identity, partial answers, obsolete questions after new input, replay suppression, blocking-question priority and active-turn routing.

After the index change, a read-only upgrade check recovered the user's actual older question with both exact offered options. Earlier superseded login prompts were filtered out. No choice or answer was submitted to that user thread.

## Verification boundary

All 58 Rust workspace tests, Clippy, six backend tests, Android lint and eleven Android unit tests pass. The tagged pipeline separately builds and validates the signed Release attachment. The original phone identity, selected user thread, host services and pairing remain unchanged; only disposable validation resources are cleaned up.
