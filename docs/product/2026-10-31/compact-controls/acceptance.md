# Alpha 12 native controls and history validation

Date: 2026-10-31. Implementation: [RFC 0011](../../../rfcs/0011-compact-desktop-controls.md), [RFC 0012](../../../rfcs/0012-history-pagination.md). Delivery target: [0.2.0-alpha.12](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.12), Android versionCode 14.

This record covers the native debug build from the tagged source, an ARM64 API 36 emulator at approximately 411dp portrait width, and a disposable Linux X11 desktop. The desktop source and decode dimensions both remained **1920×1080**, with a 60fps request. Test-only loopback TCP/UDP relays were used because the emulator could not route directly to the LAN. These are real SSH, Sunshine/Moonlight, MediaCodec, input and history integrations; they do not establish physical-phone latency, LAN throughput or WAN acceptance.

## Observed behavior

| Check | Result and evidence |
| --- | --- |
| Host manager | Add saves a new card and returns to the independent list; reopening the card edits the same record. The new port survives reconnect. Core tests verify that save-only does not switch the selected host, and explicit connection persists the selection. Host screenshots and test authorization remain local. |
| Portrait order | [Desktop → trackpad → Chat → composer](portrait-trackpad-open.png). The panel is 176dp, with a movement area and left / wheel / right row. [Closing it](portrait-trackpad-closed.png) returns 176dp to Chat while the composer bottom stays fixed. |
| Mouse buttons and wheel | The GTK fixture received both held-button movements, paired releases and continuous center-wheel events. Closing/leaving the panel releases input. A small initial move was verified to affect only the owned display; the production display did not move. |
| Direct system IME | [The desktop keyboard button opens the system IME](remote-system-keyboard.png) without a text-entry dialog. The opt-in native test exercised composition and committed text through the real 200ms scheduler, JNI and Sunshine; the remote GTK entry matched the exact controlled Latin/Chinese token. |
| Zoom and coordinates | One-finger movement after zoom pans locally without remote click/drag events. In fullscreen, after 2× zoom and a 110×40 local pan, a center tap reached source coordinates **(905,520)**, within four pixels of the inverse viewport calculation. [Zoomed view](fullscreen-zoom-pan.png). |
| Fullscreen mouse | The collapsed control has a 48dp target with a 32dp visual. It [moves into the video letterbox](fullscreen-mouse-collapsed-edge.png); [expanding at the edge](fullscreen-mouse-expanded-edge.png) keeps the separate close button visible. The expanded body is 144×112dp; total width including close is 192dp. |
| Older/newer history | A synthetic provider log contains 651 messages / 5851 source records. The actual app scrolls from the latest page to [QA0000](history-oldest.png), then explicitly loads five newer pages to reach [QA0650](history-newest.png). Pages evicted from the display remain retrievable from the source. |
| Reading position | Appending QA0650 while reading earlier messages kept the same messages visible and exposed return-to-latest. The return action restored the latest message and follow state. Continuous gestures across page boundaries exposed a scroll-cancellation dialog during development; anchor restoration now yields to active gestures and routine coroutine cancellation is not presented as an error. The repeated traversal passed after this fix. |
| Long lists | Chat, host cards, sessions and expanded tools use stable-key virtual lists. The visible history window evicts whole pages above 300 messages / 256KiB; conversation content revisions prevent video telemetry from regrouping it. This establishes bounded presentation, not a physical-device frame-time benchmark. |

The [13 controlled runtime checks](results.json) and [screenshot checksums](assets.json) contain no user chat, private keys, credentials or test authorization. All displayed conversation content belongs to the synthetic fixture. Recent offline cache is retained; full historical pages still require the original provider files online.

## Automated checks

- Rust formatting, **68 workspace tests**, Clippy with warnings denied.
- **13 host-adapter tests**, including full backward/forward history traversal, append-safe cursors, replacement/truncation, partial records and clipboard protocol tests.
- **4 publisher tests**, including signature and native-alignment requirements.
- Android debug compilation, lint and **18 JVM tests**, including geometry budgets, Unicode batching/deletion/chunking and viewport mapping.
- **8 API 36 native presentation/input tests**, plus **1 opt-in live IME test** against the owned desktop.

Geometry unit checks cover narrower and shorter budgets; the runtime screenshots above use the stated emulator size. Large-font layouts, the user's physical phone, third-party Chinese input methods and WAN responsiveness remain user/device acceptance work. Claude remains a pending provider connection; production Herdr TUI is not migrated to herdr-core by this release.

## Published APK verification

The [published Release APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.12/kite-0.2.0-alpha.12-arm64-v8a.apk) was downloaded and verified separately from local builds. The [publication record](published-release.json) identifies source **076db1285772e1ecfa4c346d01d7abf01342850a**, passing main/tag CI runs, the exact **21,046,057-byte** APK and SHA-256 **30a32c8ee1dc27948d3a78ee3bd6e5fca66dbcc4f99989926e9501de128feb67**. It verifies as a non-debuggable ARM64 Release with versionCode 14, the existing signing certificate in v1/v2/v3, and 16KiB alignment. The GitHub release is a published prerelease.

An in-place install succeeded without clearing data. The official APK reopened the same saved session and QA0650 history, reconnected with the existing device key and SSH trust, reused Sunshine pairing without a new PIN, and received/presented 1080p frames. Its trackpad and direct system IME worked, and all three saved host profiles plus the prior clipboard preference remained intact. These are emulator results; physical-phone and WAN acceptance remain outside this record.

Cleanup removed the owned session catalogue entry, synthetic provider log and cache, temporary SSH authorization, isolated Sunshine/display/GTK processes, local relay ports and owned emulator. Both phone keys and the production Sunshine service remain. The old backend-v2 file is byte-identical; the new backend-v3 remains installed for this APK. No production Herdr/Codex session was replaced.
