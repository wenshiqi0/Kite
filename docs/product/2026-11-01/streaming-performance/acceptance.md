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

## Public APK comparison

The public [Alpha 23 APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.23/kite-0.2.0-alpha.23-arm64-v8a.apk) was downloaded completely and independently verified: source `4227128550b2aba748cc757157beafbe885910b5`, versionCode 25, 21,628,490 bytes, SHA-256 `6e4045f5a71a03e9b470811efe1932d9fd213d7d09278303094ff3d59a2840e4`. Stable v1/v2/v3 signatures and 16KiB ZIP alignment pass. [Publication/CI evidence](published-release.json).

In-place installation on the same API 36 ARM64 emulator retained all 23 recorded private files byte-for-byte before launch: four host profiles, SSH identity/trust and pairings. [Upgrade evidence](android-upgrade.json). Both A/B runs used the same guarded instrumentation APK, owned animated scene, software H.264 server, relays and warmup. No model prompt was submitted.

| Measurement | Public Alpha 22 | Public Alpha 23 |
| --- | ---: | ---: |
| Source / video pixels | 1920×1080 | 1920×1080 |
| Requested FPS | 60 | 60 |
| Received FPS, roughly 14-second sample | 56.50 | 56.67 |
| Actual rendered FPS | 54.65 | 54.60 |
| UI-frame interval P95 | 16.67ms | 16.67ms |
| Maximum sampled UI-frame interval | 16.67ms | 33.33ms |
| Continuous View pointer samples | 60 in 1338ms | 60 in 1317ms |
| Independently observed host sweep motions | 1 final motion | 48 across 1382.83ms |

Video throughput is effectively unchanged in this controlled case; it is not evidence that the reported WAN video stutter is eliminated. Continuous pointer delivery during the gesture is directly demonstrated, replacing final-position-only delivery. The final mapped host coordinate matches between versions. Initialization center resets, more than 20 seconds earlier, are excluded from sweep counts.

The new decoder sample reports `c2.goldfish.h264.decoder`, hardware flag true in the emulator, queue P95 7.6ms, receive-to-render P95 12.1ms, callback delay P95 1.4ms, zero input-slot waits/timeouts/rejections and zero explicit decoder IDR requests. Eight superseded decoded outputs were skipped. These local codec statistics do not include capture, encoding or WAN transit latency and do not establish the physical phone's decoder behavior. The recent two-second FPS diagnostics and the longer core-counter average use different windows. [Exact results](results.json).

## Matching Linux deployment and cleanup

The matching public Linux archive was verified and installed on wen.linux. The running node binary matches its source-pinned manifest, node identity and all 12 recorded private files are retained, and health is `ready`. The native taskbar icon is visible; opening the existing panel and hiding it back to the tray leaves the node running. The SSH shell's default Python lacked GI during the first tray-verification attempt; the system `/usr/bin/python3` completed verification. Herdr PID 36173 and the original Sunshine service PID 775016 remain unchanged. [Deployment evidence](official-deployment.json).

Cleanup removed only the owned Xvfb scene, temporary Sunshine instance, guest/Mac relays, three ADB reverses, test APK and emulator. The original QA server pairing and rotation were restored; the updated public app and its data remain installed in that AVD. Temporary SSH authorization was removed and original authorized-key bytes restored. Five pre-existing dirty files in the primary checkout are unchanged. [Local cleanup](local-cleanup.json), [host cleanup](remote-cleanup.json).

Physical-phone Tailscale responsiveness remains a separate acceptance boundary. The phone was offline during this investigation, so its direct/relay path, active throughput and touch-to-photon latency could not be measured. This release directly fixes the reproduced continuous-input stall and adds video buffering/network mitigations with diagnostics; it does not claim a verified WAN video recovery.
