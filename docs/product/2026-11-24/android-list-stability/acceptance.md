# Android dropdown and chat list stability

The reported symptom is vertical movement of list content and dropdown menus. Two presentation defects were reproduced on an isolated Android 16/API 36 x86_64 emulator:

- Inserting a setting before an expanded permission selector disposed its positional composition and closed its popup. `permissionMenuSurvivesSettingsInsertedBeforeItsAnchor` fails before the fix. Settings and their choices now have composition keys derived from their stable IDs and values, so an open menu remains attached to the original setting.
- Markdown's `AndroidView` did not opt into lazy-list view reuse. Scrolling through 11 positions in a 48-record fixture created **46** distinct native text views. The same fixture now uses **10**. Reuse clears focus, selection, internal scrolling and attachment actions while retaining the rendered text until the next binding. A native regression verifies that an old row's attachment action cannot survive recycling, including when the next row has identical Markdown.

The baseline failures were recorded against `83b0658a`. The final implementation was rebased onto `0864b959` / v0.2.10; its Rust JNI library and Kotlin bindings were regenerated together.

## Validation

- Debug app and instrumentation assembly, **91 JVM tests** and debug lint pass.
- **24 native tests pass**, including the complete native presentation suite, refreshed settings, an open model menu during 24 chat updates, view reuse, streaming reply retention, history anchoring, inline attachments and the current session sidebar.
- The streaming regression checks every draw for a visible, nonzero-height reply and an already positioned tail. It also covers focused selectable text, paused history, explicit return to latest and reply shrink/completion. This continuously animated fixture awaits actual drawn text and geometry instead of global UI idleness.
- Across five viewport changes, every measured reply-to-viewport gap remains **52px**. Concurrent viewport changes and appended/removed footer rows produce **15 drawn tail gaps of 0px**. These controls also passed before the repair; the tail-following algorithm was not changed.
- The existing Android presentation CI now runs the new list/menu regressions and triggers for the relevant settings and chat viewport sources.

Release preparation also corrects the shared client's passive-quota fixture: connection readiness does not imply that its asynchronous session catalogue has finished loading. The fixture now waits for its saved conversation, selects it explicitly and verifies that reading quotas preserves both the host and that nonempty session selection. The shared Rust component suites and three consecutive reruns of this regression pass locally.

[Machine-readable results](validation.json) include the native test names and measured geometry. [Open menu during chat updates](dropdown-during-chat-updates.png) and [streaming reply](streaming-chat-stable-tail.png) are synthetic fixture captures.

This validates the reproduced presentation paths. No physical handset or real user conversation was connected, so confirmation of every reported phone-side movement remains separate. The local debug APK is a test artifact; public delivery uses the merged source tag and GitHub release workflow.
