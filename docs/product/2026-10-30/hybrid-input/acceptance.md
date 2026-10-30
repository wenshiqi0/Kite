# Alpha 11 Android interaction acceptance — 2026-10-30

Candidate build: **0.2.0-alpha.11 / versionCode 13**. Validation used the dedicated ARM64 Android 16 / API 36 emulator, SSH and an isolated 1920×1080 Xvfb/Sunshine desktop on the Linux host. The user's normal desktop, Sunshine service and active conversation remained running. No validation prompts were sent to that conversation. These are emulator and controlled-host results, not physical-phone latency or WAN acceptance.

## Sidebar and rich text

The close button, scrim and Back each dismiss the session drawer without selecting a session. A swipe from the closed left edge does not open it. Desktop input is blocked under the drawer, and held buttons are released.

Question choices and descriptions now use the same Markdown parser as replies; the selected wire value remains the original text. Native tests verify bold, emphasis, code and escaped literal stars. Streaming leading strong spans render before their closing marker arrives, while code and source copied from history stay exact. Approval explanations use Markdown, with the original command and directory kept separate and monospace.

## Hybrid input

The floating mouse and direct canvas touch are available together. Real GTK event counters verify balanced left/right presses, continuous wheel events, and held-button dragging with a second finger in either the movement area or the video canvas. The Compose/native-view boundary needs explicit routing for a canvas finger whose gesture began on the floating mouse; the runtime regression covers both buttons and prevents an extra click.

An off-center touch after **250% zoom** mapped to source pixel **(858, 540)**, versus expected **(858.73, 540)**. See [coordinate evidence](zoom-input.json). Relative mouse movement is independent of local zoom. The [controlled screenshot](hybrid-zoom.png) shows the candidate UI with test content only.

## Automatic clipboard

The UI has one persistent **同步剪贴板** setting, initially off. There are no manual send/fetch actions. On the selected, focused operating desktop:

- A phone system-clipboard copy arrived on the remote desktop without another action.
- A remote Chinese copy imported into Android and pasted into a local draft. The draft was cleared and never sent to a model.
- Native Ctrl+V transferred and pasted the exact Unicode test token in the remote GTK entry.
- Watch mode paused incoming synchronization. Turning the setting off stopped a fresh outgoing phone copy.
- The off preference survived the next APK update. Core hash tracking prevents imported text echoing back.

The first sync prefers existing phone text; simultaneous fresh copies prefer the phone. Non-text selections are not converted to arbitrary text. This Alpha covers UTF-8 text up to 256 KiB; clipboard images/files, full Wayland/macOS host integration and background Android clipboard access are not accepted here. Linux tests bind to the configured Sunshine process and its display; a pointer isolation check also verified that the production display did not move.

## Validation

- Rust: 60 workspace tests, Clippy with warnings denied, and formatting pass.
- Python: 10 host-adapter tests and 4 publisher tests pass, including actual bounded Unicode pipes, oversized input and unavailable/non-text selections.
- Android: 13 host JVM tests, Debug lint/assembly and 6 native device tests pass.
- [Recorded runtime cases](results.json) cover sidebar, mouse, coordinate mapping and clipboard behavior. Tagged CI separately verifies the Release variant, stable certificate, all three APK signature schemes and 16 KiB ZIP alignment before publishing.

The independent [herdr-core experiment](../herdr-core-migration.md) verifies preserved provider context after terminal reattachment. Production runtime migration, macOS/Linux desktop clients and multi-machine MCP remain later stages in the [implementation plan](../../../implementation-plan.md).

## Published attachment confirmation

The [public Alpha 11](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.11) attachment was downloaded after both main and tagged CI passed. Its **20,906,418 bytes** match the manifest SHA-256 `f6ecea40f69b58533a2f0a37bc8ba905453c6700f9420d3c4581f8ce78653bcf` and source commit `1bd806f15b0c49842f857b5128151a028fca052d`. Local verification independently passed v1/v2/v3 signatures with the existing Alpha certificate, Release metadata and 16 KiB ZIP alignment.

That exact non-debuggable APK installed over the candidate, reconnected over SSH and reopened the paired desktop without reauthorization or another PIN. Its clipboard preference survived, and its automatic phone→desktop and Chinese desktop→phone copy/paste paths passed again after enabling the single switch. This confirms the released bytes on the API 36 emulator; it does not claim a physical-handset run. [Machine-readable delivery evidence](published-release.json).

Owned test services, display, emulator, helper apps and temporary SSH authorization were removed. Both phone public keys remain authorized; the original Sunshine and provider thread remain live. The host's private `xclip` dependency remains installed for the feature.
