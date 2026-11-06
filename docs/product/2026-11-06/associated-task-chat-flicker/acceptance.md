# Associated-task expansion and chat position

This client-only fix is stacked on [PR #19](https://github.com/wenshiqi0/Kite/pull/19), the Alpha 86 source `b3a54ad66e06e5fe4b65419abd08c2b9d73a89fa`.

Expanding associated tasks shrinks the chat viewport. Previously the lazy list first retained its old reading anchor, drawing the latest reply **252px lower**, then `ChatTailFollow` corrected the scroll on the next frame. Four expansions reproduced this exact displacement in an Android draw listener.

The workspace requests its tail item before the task panel toggles, using `LazyListState.requestScrollToItem` to apply the anchor during the next remeasure. This happens only while following the latest reply and with no active scroll. History readers keep their existing list anchor; normal gestures and the asynchronous follower remain in place.

`AssociatedTaskPresentationTest` runs the actual workspace with isolated synthetic display records, restoring the native mirror afterward. The expansion case checks every draw through eight toggles, retaining the same native Markdown view, complete visible text and screen position. The history case uses a real touchscreen fling, then checks native-view identity and its offset within the resized viewport through four toggles. Native coordinates avoid Android accessibility's clipping of partially visible text bounds. [Measured positions](native-validation.json).

All nine native cases execute on an isolated Android 16/API 36 x86_64 emulator: the two expansion regressions plus compact list navigation, read-only messages/tools/results, refresh position, direct and nested flings, workspace tail settlement and downward dismissal. Release JVM tests (**83**), Release lint and publisher tests (**9**) pass. The emulator uses matching Rust/UniFFI artifacts; no host, provider turn or saved conversation is used. Physical-handset acceptance is separate.

Delivery uses [Alpha 87](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.87), versionCode **89**, through `android/scripts/release-alpha.py`. The tagged ARM64 Release build and its public signature/checksum verification are separate from native x86_64 execution. No server interface changes are required.
