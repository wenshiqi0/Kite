# Android streaming chat stability — 2026-11-03

The reported symptom is flicker in Chat, especially while a reply is generated. Inspection found that response-phase changes replaced the selectable Markwon text even when the rendered Markdown was identical, while every tail-message/phase update scheduled a forced list seek after two frames.

The native regression `responsePhaseChangesKeepTheSameSelectableMarkdown` fails against the original implementation: identical visible text is replaced on the first phase change. The fix retains identical rendered Markdown, including completion of an already displayed strong span, and preserves final CommonMark behavior for unfinished markers. Unchanged text colours no longer trigger an invalidation.

`ChatTailFollow` now observes measured list geometry. It seeks once on entry or an explicit return to latest, then scrolls only for actual overflow or appended content. A paused reader is checked immediately, including the interval before recomposition cancels the effect.

## Local validation

- Android debug assembly, instrumentation assembly, `lintDebug` and **78 JVM tests** pass.
- Android 16 / API 36 native execution: **13 tests pass**. This includes all `NativePresentationTest` cases and the production tail follower in `streamingRepliesRetainTheirViewAndFollowOnlyWhileReadingTheTail`.
- The synthetic streaming fixture waits for each of **24 individual Android draws**, matching text and measured height. It checks the same native view remains visible with nonzero height, retains text across six phase changes, preserves an earlier history position during an update, resumes explicit following and settles after reply shrink/completion. The [final capture](streaming-chat-stable-tail.png) shows update 23 and the response footer at the tail.
- The existing CJK native test now attaches its measured TextView to a parent and accounts for Markwon's inline-code padding characters; its original fixture failed before it could exercise the code-boundary assertion.

These local UI checks reuse the unchanged Core's existing paired JNI libraries and generated Kotlin bindings from source matching `origin/main` in the Rust and Android controller paths. No provider turn or remote desktop is used. The official tagged Release build independently regenerates its native library and bindings. Native-view/draw checks and the screenshot do not establish physical-handset flicker or GPU frame timing; handset confirmation remains pending.

The Alpha delivery is based on the already published Alpha 70 source, preserving its host/client features. Only the Android client needs updating for this fix.

## Published Alpha 71

[Alpha 71](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.71), versionCode **73**, was published from `7eb274ecd890668dd78ac684663d4aa6610b22ee`. Both the [fix PR CI](https://github.com/wenshiqi0/Kite/actions/runs/41590441331) and [tagged build/publication](https://github.com/wenshiqi0/Kite/actions/runs/41590444506) pass. All five public assets were downloaded independently; sizes, GitHub asset digests and `SHA256SUMS` match. The primary and compatibility APKs retain Alpha 70's signing certificate, verify on API 23 and API 36, have matching application payloads and pass 16 KiB native ZIP alignment. [Exact public verification](public-verification.json). Physical-handset installation and flicker confirmation remain pending.
