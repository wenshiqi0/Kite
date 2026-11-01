# Question attention and latest-message navigation

## Findings in the Android client

An unanswered asynchronous question can coexist with provider `Thinking` activity. The UI previously used that activity directly for the chat feedback row, so a question form could appear beside an animated thinking label. The native presentation now gives pending question/approval attention priority, shows an explicit non-animated waiting label, and immediately reflects the selected question in the drawer badge. The provider's execution and blocking/nonblocking semantics remain separate; the existing asynchronous-question hint still applies.

The old tail logic treated the bottom overflow of any sufficiently tall visible history row as if it measured the distance to the actual end. That replaced the accumulated distance with a short value; a downward scroll could then hide the button at an old user answer. A temporarily false `canScrollForward` could also reset follow state without checking which part of history was visible.

## New navigation rule

The button appears after 240 dp instead of 120 dp. Once shown, scroll deltas and interior-row measurements cannot hide it. An interior row's overflow is only a lower bound that may show the button. Exact end distance is accepted only from the actual final content row plus the shared 8 dp row spacing/8 dp tail spacer, or from that final spacer itself. The hide margin increases from 48 to 96 dp. Auto-follow can resume only with a valid end measurement, idle scroll state and no forward scroll; an interior or transient layout cannot resume it.

This logic is independent of message role. Historical user answers, assistant replies, question cards, approval cards and response feedback all contribute to the same native list geometry. The final spacer has a stable key. A growing reply still shows the button even before its later rows enter the viewport.

## Validation

Local checks pass: 118 Rust workspace tests, Clippy with warnings denied, 8 publisher tests and 27 host adapter tests. JVM cases cover tall interior replies, downward movement through history, end/spacer equivalence, changing footer row counts, growing offscreen replies, invalid layouts, temporary cannot-scroll state, and question/approval priority. Native presentation sources check the explicit waiting label and the increased drag threshold. Native/device execution is separate from compilation.

Source references for the prior behavior: [Alpha 33 scroll observer](https://github.com/wenshiqi0/Kite/blob/v0.2.0-alpha.33/android/app/src/main/java/io/github/wenshiqi0/kite/ui/KiteWorkspace.kt), [old margin state](https://github.com/wenshiqi0/Kite/blob/v0.2.0-alpha.33/android/app/src/main/java/io/github/wenshiqi0/kite/ui/ChatTailMargin.kt), [feedback phase selection](https://github.com/wenshiqi0/Kite/blob/v0.2.0-alpha.33/android/app/src/main/java/io/github/wenshiqi0/kite/ui/ResponseFeedback.kt).

The [Android PR check](https://github.com/wenshiqi0/Kite/actions/runs/41307023416) passes Release compilation, lint, JVM regressions and debug instrumentation-source compilation. It does not establish phone gesture execution.
