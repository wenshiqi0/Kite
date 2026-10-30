# Android Alpha 8 native presentation — 2026-10-30

Validation uses an Android 16 / API 36 ARM64 emulator. These results establish emulator behavior, not a physical NX769J acceptance run. The Linux desktop remains at 1920×1080; no test messages were sent to the user's active conversation.

## Split first frame

Alpha 7 reproduced a blank desktop immediately after entering split. Android's view hierarchy reported a 1080×764 `VideoSurface` containing a 1×1 `SurfaceView` at (539,381). Dragging the divider caused a 1080×608 child to appear. The same decoder had already received a 1920×1080 stream, so the failure was presentation geometry.

The new native measure/layout path produces a 1080×608 child at (0,78) during the first layout of that 1080×764 viewport. A real desktop is visible before touching the divider. Device tests also exercise alternate split heights, landscape dimensions, source aspect changes, zoom 1–4, and PiP fit. Rendered source points match the existing `DesktopViewport` coordinate system.

A live split → fullscreen → floating → split cycle, followed by three divider drags, retained the same decoder. The native child also had the correct dimensions immediately after returning to split.

## Rich replies

`MessageCard` previously passed assistant Markdown directly to Compose `Text`. Assistant replies and public progress now use a shared Markwon renderer. User messages and raw tool details keep their existing presentation. Rebinding unchanged text retains the same selectable buffer; streaming changes remeasure both growing and shrinking replies.

Four device tests in `NativePresentationTest` validate native Surface geometry, styled spans and streaming layout, permitted link spans, and a real attached view containing a narrow three-column table. The [on-screen renderer fixture](markdown-onscreen.png) contains only test content. Its table wraps and settles through native invalidation without touch or manual resize. The fixture temporarily overlays the emulator activity and is removed by the test; it is not a production screen or a model-generated answer.

Text can be selected, and the reply copy button preserves the complete original message, including table contents. Web and mail links can open through Android. Local paths and unsupported schemes remain readable labels without a misleading clickable link. HTML and automatic remote image fetching are not enabled; uploaded attachments retain their existing component.

## Checks

- `:app:lintDebug` and all eleven `:app:testDebugUnitTest` cases pass.
- `:app:assembleDebugAndroidTest` and four on-device tests pass.
- The release pipeline separately builds the signed Release APK, runs Release lint/unit checks and the workspace's Rust/host-adapter checks, then publishes an immutable Alpha prerelease.

Original product references remain under `docs/product/2026-10-29/`. Host services, persistent sessions and the phone's SSH authorization are unchanged.
