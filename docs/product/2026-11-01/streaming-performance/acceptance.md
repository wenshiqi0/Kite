# Streaming responsiveness — Alpha 23

## Report and confirmed scope

The user reported both video and mouse stutter while controlling Linux through Tailscale from another network. They approved continuous frame-window pointer/wheel delivery, retaining a 200ms text buffer. Source pixels and the existing fullscreen mouse / split-view trackpad remain unchanged. Host history, identity, trust, pairing and clipboard formats are unchanged; Host API 1 / adapter protocol 5 are unchanged.

## Findings

- Alpha 22 fullscreen input restarted its 200ms quiet timer for every event. Continuous motion could defer all network input until the gesture ended. `RemoteInputBuffer` now uses an absolute deadline: 16ms for pointer/wheel, 200ms for text. Buttons and keys immediately flush preceding events. Cancellation discards unsent events and releases remote state.
- The inspected Linux desktop was 1920×1080 at 50Hz. Sunshine selected AMD VAAPI H.264; CPU, available memory and GPU readings did not show load pressure while the stream was disconnected. Thirty LAN gateway pings had a 4.69ms P95. These are idle/control measurements, not an active streaming benchmark.
- The phone was offline during the Tailscale probe. Its last relay region was Tokyo and host netcheck reported variable destination mappings. This does not establish that the affected session used DERP or quantify the phone's throughput.
- Native media production sources were unchanged between Alpha 17 and Alpha 22. The new decoder work addresses identified buffering risks but does not establish a newly introduced Alpha 22 decoder regression.

## Changes

`VideoRenderer` prioritizes available hardware decoder capabilities, drains decoded output before feeding more input, retains compressed reference frames through transient slot pressure and uses a 50ms recovery budget with 250ms recovery-request spacing. Superseded decoded output may be discarded to catch up. Frame callbacks run off the UI thread. Priority, operating rate and low-latency hints are configured without changing the source resolution.

The Rust stream configuration caps initial video bitrate at 6000kbps for Tailscale IPs (or an unresolved address); LAN budgets remain unchanged. This preserves dimensions and FPS and is not adaptive congestion control. Six Mbps does not guarantee a relayed path has enough throughput.

The existing diagnostics display bounded numeric receive/submit/render FPS, queue/render/callback P95, codec identity and buffer/recovery counts. They do not collect frame bytes, keyboard text or account data.

## Validation before publication

- Rust workspace: 92 tests pass; clippy with warnings denied and formatting pass.
- Android Release and instrumentation Kotlin compile; Release lint passes.
- Thirteen focused JVM tests pass, including continuous 16ms motion windows, fixed 200ms text, click/key ordering, cancellation, overflow, retry/recovery timing and bounded statistics.
- Public Alpha 22 baseline on the owned Android API 36 ARM64 emulator and isolated 1080p moving test scene: received 56.5fps, rendered 54.7fps; UI-frame interval P95 16.67ms, maximum 16.67ms. The server used software H.264 on a separate Xvfb display, with no user desktop content or model prompt. The same harness sent 60 pointer samples across 1338ms; the host observed only one final sweep motion, after the old quiet window. Initialization center resets are excluded. See [baseline numbers](baseline.json). This is a controlled local baseline, not a Tailscale measurement.

The final public-package comparison, installation retention and matching Linux deployment are recorded after publication. Physical-phone Tailscale responsiveness remains a separate acceptance boundary.
