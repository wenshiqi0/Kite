# Fullscreen virtual pointer correction

Date: 2026-10-31. Target: **0.2.0-alpha.14**, Android versionCode **16**.

Alpha 13 reproduced the mouse-shaped buttons but only moved the floating control. The buttons still depended on the previous canvas position. The supplied reference requires a visible arrow and one hotspot shared by movement, buttons and scrolling. This record corrects that interaction; the earlier Alpha 13 transcript and rendering measurements keep their original scope.

## Behavior

Expanding the mouse now shows a native arrow above the video. Its hotspot is stored in source desktop pixels. The lower grip moves both the control and pointer; left/right buttons and the wheel synchronize that hotspot before sending their event. Holding a button and dragging also moves the pointer with the button down. Direct canvas touch continues to update the same pointer. Closing the mouse releases held buttons and hides the arrow; moving the collapsed icon only repositions the launcher.

The [expanded fullscreen screenshot](fullscreen-pointer.png) follows the supplied layout, including a close button to the right and a clear arrow to the left. The [zoomed screenshot](zoomed-pointer.png) uses the same source hotspot after 2× zoom and local panning. The control's screen bounds do not limit subsequent pointer movement. Source edges, letterboxing, source-resolution changes and layout changes use the existing `DesktopViewport` transform. Layout-only gestures do not send remote input.

The relative trackpad remains confined to portrait split view, below the desktop and above Chat. This correction changes native geometry, rendering and input collection; it changes no backend, persistence format or FFI contract. History, device identity, host records, trust and pairings retain their existing contracts.

## Actual input evidence

The opt-in [native UI test](../../../../android/app/src/androidTest/java/io/github/wenshiqi0/kite/LiveVirtualMouseTest.kt) targets only the loopback QA host and isolated Sunshine port. It taps real Compose buttons, drags the grip and a held button, uses the wheel, then zooms/pans and clicks again. It records the visible pointer's source coordinates independently of the host's received events.

The [coordinate comparison](coordinate-validation.json), [pointer targets](pointer-targets.json) and [host input log](host-input-events.json) verify **5 presses, 5 paired releases, 7 wheel events and 7 held movements**. Every button and wheel target agrees with the visible arrow within **2 source pixels**. The 2× zoom/pan click reached approximately **(999,395)** on the 1920×1080 desktop. Dragging a held button by −100/−40 screen pixels moved its logical hotspot by the corresponding source delta; moving the control no longer subtracts from the finger movement.

The first controlled pass exposed stale accessibility bounds in the test driver and then a real drag-delta error in the control. The test now refreshes its accessibility cache before locating a moved button and asserts the complete drag distance. Acceptance uses the final passing run and its corresponding event log.

[Native checks](native-checks.json): **9 real Android presentation tests**, **1 controlled mouse UI/JNI test** and **15 standalone pointer/viewport/motion tests** passed. The presentation suite measures and draws the actual cursor view, verifies its hotspot and checks visibility across close, disabled gestures and picture-in-picture. The local Release lint check also passed. Local Gradle still lacks its JVM worker bootstrap; normal Release JVM checks remain required in the trusted tagged CI build.

## Validation boundary

The runtime is an owned ARM64 API 36 emulator and an isolated Linux X11/Sunshine desktop. These results establish rendering, coordinate alignment and button lifecycle on that setup. Physical-phone latency and third-party input methods are separate acceptance work. Screenshots contain only the synthetic fixture; the user's reference screenshots and conversations are not published.

The immutable public APK, signing/installation checks and cleanup results are recorded after publication.
