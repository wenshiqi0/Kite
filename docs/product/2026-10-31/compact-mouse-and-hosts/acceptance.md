# Compact mouse, tactile wheel and simple host cards

Date: 2026-10-31. Target: **0.2.0-alpha.15**, Android versionCode **17**.

## Product changes

The fullscreen mouse body changes from 128×144 to **104×112 dp**, reducing its visible area by approximately **37%**. The native arrow height changes from 28 to **20 dp**. The separate 48dp close target and source-pixel pointer remain. The [fullscreen screenshot](fullscreen-compact-mouse.png) and [zoomed view](zoomed-compact-mouse.png) use the controlled 1920×1080 desktop. The trackpad remains below the desktop in portrait split view.

The wheel now supports upper/lower half taps for one notch in either direction and continuous dragging. [Pressed-state rendering](wheel-pressed.png) shows highlighting and the direction cue. Mouse buttons and the wheel request a tactile press; accepted scrolling accumulates tactile detents and limits their frequency to one every 40ms. Cancelled or intercepted gestures do not synthesize a final tap or scroll.

The [host card preview](host-card-states.png) renders the real `HostConnectionCard` component with five synthetic connection states. Cards show the host name, a status and Connect/Enter/Retry, with editing separate from connection. Usernames, ports, backend paths, stream endpoints and fingerprints are absent from the list. The full host panel was also exercised with the actual connected QA host: its gear opens clipboard settings, public-key access and diagnostics. Error details start collapsed. See the [panel result](host-panel-runtime.json).

Status comes from Core's actual connection result. An unconnected saved profile is labelled unconnected, not inferred offline or online. This revision does not add background presence scanning. Configuration, identity, trust, pairing, clipboard preferences and full conversation data retain their existing formats.

## Verification

The [native checks](native-checks.json) record **11 Android presentation/control tests**, **1 controlled live mouse test**, **19 standalone pointer/detent tests** and Release lint. The [control tests](../../../../android/app/src/androidTest/java/io/github/wenshiqi0/kite/ControlsPresentationTest.kt) inject real Compose touch events and check upper/lower wheel taps, continuous scroll, press state, feedback requests, cancellation, hidden card details and independent editing. The tests contact no host through their card callbacks. The separate live test uses the guarded QA host and Sunshine port.

The [host event comparison](coordinate-validation.json), [pointer targets](pointer-targets.json) and [raw events](host-input-events.json) confirm **5 mouse presses, 5 matching releases, 11 wheel events and 9 held movements**. Wheel taps begin with up then down. All button and wheel positions agree with the arrow within **2 source pixels**, including after 2× zoom/pan. The actual control measured **104×112 dp**. Held dragging preserves the complete finger delta.

Android's vibrator-manager history independently recorded **8 VIRTUAL_KEY press requests and 1 CLOCK_TICK request** from the real app, through `ViewRootImpl#performHapticFeedback`, with TOUCH usage. The [system dispatch record](haptics-validation.json) is distinct from the control test's feedback callback assertions. The emulator confirms the platform dispatch path; it cannot establish how a physical phone's motor feels.

Haptics follow the [Android platform API](https://developer.android.com/develop/ui/views/haptics/haptics-apis), respect system touch-feedback settings and add no vibration permission. Phone-specific strength and WAN response remain outside this emulator/X11 validation. The supplied UU screenshots and private host/session content are not published; screenshots here contain only synthetic fixtures.

Normal Release JVM/lint/signing checks and public-attachment installation are recorded after the trusted Alpha publication. Local dependency resolution uses the same pinned public artifacts via the existing temporary read-only mirror; no project dependency versions or CI gates are changed.
