# Complete transcript and dark workspace validation

Date: 2026-10-31. Design: [RFC 0013](../../../rfcs/0013-complete-transcript-and-dark-workspace.md). Target delivery: 0.2.0-alpha.13, versionCode 15.

Alpha 12 mixed a cropped data window with virtual rendering. This revision keeps the conversation independently of its rendered window: background synchronization and persistent caches belong to Rust, native code receives versioned message edits, and scrolling only changes local presentation. The previous Alpha 12 results retain their original scope and do not establish acceptance of this revised behavior.

## Data and rendering evidence

- The controlled provider file contains **10,000 logical messages / 90,001 source records**, including a **3MiB tool result**. Android instrumentation verified that the complete message collection and row projection contain 10,000 entries.
- After scrolling to a normal reading position, the real Android view tree contained **5 native Markdown views**, not 10,000. The [measurement](long-transcript-validation.json) also confirms that real video-statistic updates retained the first, middle and last message objects and did not change the conversation revision. This is a view-allocation result, not a physical-phone frame-time benchmark.
- The [independent cache check](cache-validation.json) verifies all 10,000 records, the first/last IDs and the complete 3,145,728-byte tool result. The sessions catalogue contains no inline message bodies.
- With the owned provider file temporarily unavailable, a fresh app launch reopened saved history and scrolled to earlier messages locally. No older/newer-page controls or data eviction are involved. [Cached view](cached-history.png), [local scrolling](local-scroll.png).
- Appending QA10000 while reading earlier messages preserved the same visible text and pixel bounds. Return-to-latest then displayed the new message. The [runtime checks](runtime-checks.json) record these separately from source/JVM tests.
- [Large tool output](large-tool-output.png) keeps its complete data and renders bounded 1024-character chunks in a virtual list, avoiding one enormous paragraph layout.

## Interaction and colour

[Fullscreen mouse](fullscreen-mouse.png) follows the supplied mouse reference: left/right buttons, center wheel and a lower positioning grip, with a right-hand operation rail. It has no relative trackpad area. The [collapsed icon](fullscreen-icon.png) is draggable. Edge expansion keeps close reachable and moving the control does not send remote input.

The [portrait split trackpad](split-trackpad.png) is below the desktop and above Chat. Both held mouse buttons, paired release and wheel input reached the owned GTK canvas. Closing the 176dp panel returned that space to Chat while the composer bottom stayed fixed.

The source and decode dimensions remained **1920×1080**. After 2× zoom and a 110×40 local pan, a center tap hit source **(905,520)** within four pixels; one-finger panning did not emit a remote drag. The direct system IME test delivered the exact controlled Latin/Chinese token through the real 200ms buffer and Sunshine path.

Colours are shared across Compose, native Markdown/code/tables, cards, sidebars, questions, approvals and system chrome. The actual screenshots above show the dark palette. The remote GTK window is an owned fixture; Kite does not change the colour scheme of a user's captured desktop.

## Automated scope

Recorded checks (JVM tests also passed in the trusted tagged Release build): **75 Rust tests**, **15 host-adapter tests**, **4 publisher tests**, **22 Android JVM tests**, **8 native presentation tests**, plus **1 controlled long-transcript test** and **1 controlled direct-IME test**.

Rust workspace tests and Clippy cover full collections, source-order/cursor coverage, append gaps, immutable telemetry, one-record tail deltas, late-context resets, complete cache migration and preservation on empty/failed reads. Host-adapter tests cover 10,000-record traversal, duplicate text, large inline images, a 3MiB text record, explicit rejection of oversized records and compressed transport. Android JVM checks cover incremental native rows, stable tool-group keys, Unicode chunks, viewport mapping and IME batching. API 36 tests cover native rendering plus the owned long-transcript and direct-IME integrations.

Local Gradle dependency resolution needed a temporary read-only mirror of the same pinned public artifacts after repository stalls. This does not change project versions or omit compilation/lint/tests. Hosted Release CI uses the repository's standard configuration.

## Boundaries

The runtime is an owned ARM64 API 36 emulator at approximately 411dp portrait width, an isolated Linux X11 desktop, and private test-only TCP/UDP relays. It is not physical-phone or WAN latency acceptance. Claude remains a pending provider connection. This release does not migrate production Herdr TUI sessions or ship macOS/Linux desktop clients or multi-machine MCP.

Transport batches are bounded, with up to 4MiB decompressed per response. This does not cap total conversation size. A single record beyond the transport boundary reports failure and preserves existing content instead of silently skipping it. The original provider files remain authoritative and unchanged.

## Published APK verification

The [publication record](published-release.json) identifies source **dc099e70d584d25e090abc64169dc7040f879bd1**, successful main/tag CI runs, and the exact **21,385,599-byte** Release APK with SHA-256 **33f256fee97dab331acf523cb79d5c0cffca9c53ab86662473bdc890bcb2619c**. The downloaded public asset passed versionCode 15, non-debuggable ARM64 Release, stable v1/v2/v3 certificate, checksum and 16KiB alignment checks.

An in-place installation retained the complete saved history through QA10000, all four saved host profiles, device identity, SSH trust, Sunshine pairing and the clipboard preference. The official APK reconnected without a new PIN and presented 1080p frames. Its [fullscreen mouse](release-fullscreen-mouse.png), [split-only trackpad](release-split-trackpad.png), dark theme and direct system IME were exercised separately from local builds. A final cold native regression passed against the appended **10,001-record** fixture and verified the bounded view allocation; the emulator was returned to the exact public APK afterward.

The owned provider log, catalogue record, control cache, temporary SSH authorization, Sunshine/GTK/Xvfb instances, relay ports and emulator were cleaned up. Both phone keys and production Sunshine remain; backend-v2 and backend-v3 are byte-identical, and backend-v4 remains installed for the new APK.

The late local rerun hit a missing Gradle test-worker bootstrap class after dependency-cache recovery; no product checks were disabled. The hosted tagged build passed the normal repository configuration, Rust/host/publisher checks, Release lint and all JVM tests. Native and published-APK runtime checks are recorded above. Physical-phone and WAN responsiveness remain unverified.
