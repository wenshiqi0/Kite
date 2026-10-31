# Anchored fullscreen input and Linux host node

Date: 2026-10-31. Target: **0.2.0-alpha.17**, Android versionCode **19**.

## Fixed pointer and quiet input

The arrow is now stored in screen geometry with a fixed offset from the mouse body: 24dp left of its body and aligned to its top. Source targeting is recomputed through `DesktopViewport`. Zoom and pan do not separate the rendered arrow from the controls, and direct canvas input does not relocate the control's arrow. Continued movement at a control boundary pans the local image; letterbox/source boundaries can move beneath the fixed hotspot. Source desktop resolution remains 1920×1080 in the controlled setup.

The [normal view](anchored-mouse.png) and [zoomed view](zoomed-anchored-mouse.png) come from the actual Android UI. The guarded live test checks the screen offset before zoom, after zoom and after pan, along with real host click/drag/wheel input. The native renderer test checks the actual cursor plane and its source projection. This supersedes Alpha 14/15's source-stored cursor behavior.

Fullscreen operation uses a **200ms quiet window**, not periodic 200ms delivery. Adjacent positions, wheel movement and committed text coalesce; button/key transitions and drag start/end targets preserve order. IME composition stays local and committed text enters this same window without another 200ms wait. Watch, blur, background, layout/target change and stream destruction cancel unsent input and release remote state. Delivery failures are not replayed.

The [real input timing check](quiet-input-validation.json) injects a **1.4-second** controller drag. The owned host's event count stayed **44 → 44 → 44** while movement continued, then became **45**, with only one final motion delivered after the stop. Clock-controlled tests separately cover continuous sampling, clamped-edge activity, click/drag/key barriers, wheel coalescing, bounds, cancellation and stale layout events.

## Linux host activation

[RFC 0014](../../../rfcs/0014-linux-host-node.md) implements the first desktop host stage: a persistent Rust `kite-node`, native GTK panel, user-level installer and MCP stdio queries. New APK chat, Herdr API, session/history and attachment operations traverse Node over existing SSH. Node executes the matching `backend-v5.py`. No Python-direct fallback is added. Existing Herdr runtime/provider ownership is reused; production herdr-core migration is not performed.

The [deployed node check](node-service-validation.json) verifies API 1, adapter protocol 5, a 0600 socket/0700 directory, and preserved original Herdr/Sunshine process identities and backend-v2/v3/v4 bytes. The [native panel](linux-host-panel.png) was rendered and closed on the owned X11 display; the real systemd node retained the same PID and remained queryable through a fresh SSH connection. The configured services are external and are not stopped by the panel.

Real bridge validation initialized a long-lived proxy against an existing Codex owner and read its eight-model catalogue. A **1MiB binary upload** matched its SHA-256 and was removed afterward. Compressed history passed intact through the framed transport. MCP initialized, discovered its two read-only tools and queried actual status/session data. Invalid tool arguments and command/executable selection are rejected in offline tests.

The phone created a **new, owned Codex conversation** and received `KITE_NODE_ALPHA17_OWNED_OK` through the node using `gpt-6-luna` / low reasoning. Its [record](node-chat-validation.json) distinguishes this deliberately submitted turn from the read-only bridge probes. No test prompt was sent to an existing user conversation. Node's request counters increased while Android used it.

[Checks](checks.json) include Rust/domain, native input/UI, adapter/publisher/package and lint validation. This stage delivers Linux host services and a host panel; full desktop remote-client UI, durable multi-machine message/task routing and production herdr-core migration remain pending. Physical-phone tactile strength, WAN latency and other host platforms are separate acceptance boundaries.

The immutable Android/Linux tagged build, public package installation, provider/context retention across node update and final cleanup are recorded after publication.
