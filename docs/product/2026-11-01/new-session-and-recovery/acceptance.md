# New sessions, desktop audio and recovery — Alpha 26

## Report and evidence

The user confirmed the latest Alpha 23 client and reported every new session showing a sync error, unexpected terminal output and an unstable desktop connection. They then requested desktop sound playback in the same delivery.

A controlled run of the public Alpha 23 APK reproduced `composerReady=true`, `historyFailed=true`, and an automatically visible terminal entry/card during fresh-session preparation. No model prompt was submitted in that baseline. The live owner and loaded thread existed before the provider wrote a history file. Thus an empty new session was misclassified as a history read failure; a successful chat channel did not clear that presentation error. [Baseline](baseline.json).

Separately, the production Sunshine journal at 09:30:17 +08:00 records a PulseAudio `cleanup_time_events()` assertion and exit status 134, followed by an automatic restart at 09:30:28. A later connection at 09:32:31.892 ended at 09:32:34.419 without a second observed process abort. The later disconnect cause remains unconfirmed; the phone being offline at the subsequent probe does not establish that it used DERP or why it disconnected.

## Changes and boundaries

- Android receives encrypted Moonlight Opus packets through the dedicated media bridge, decodes 48kHz stereo with MediaCodec and plays PCM with AudioTrack. Audio uses its own bounded worker/queue, handles local mute and audio focus, and releases resources with the stream. Audio failure does not fail video. Numeric diagnostics retain counts only.
- The workspace menu exposes a persistent local sound toggle. Host local playback stays enabled; connecting no longer requests a default audio-device swap. This mitigates the observed sink-switch/restore path and is not a repaired PulseAudio library.

- A successful history-source lookup with no provider file reports an explicit pending-source state. It does not fabricate an empty completed history or erase cached/local messages. Transport, command and decoding failures remain failures with bounded diagnostic details.
- Native Chat presents a clean new-conversation hint. It no longer automatically exposes terminal contents or a terminal link during normal session preparation. The deliberate terminal/diagnostics menu remains available, as do supported questions and approval actions.
- Transient chat initialization failures can retry after five seconds. Historical gap reads use a capped 2/4/8/16/30-second retry cooldown; manual retry resets it. These retries do not replay user prompts, answers, approvals or desktop input.
- Disconnected, starting, pairing, stopping and failed desktop states have distinct labels. Idle offers a reconnect action instead of a spinner; the split toolbar displays actual state rather than unconditional `live`. SSH closure marks an active desktop failed and releases input.
- Desktop discovery, launch and handshake waits are bounded independently. After startup, missing presentation for ten seconds becomes an explicit stream failure; initial frame presentation retains its 15-second budget. User PIN confirmation stays separate.

Stored history, host profiles, device identity, trust, pairing and preferences keep their formats. The FFI library and bindings are regenerated together for pending-source/details metadata. Host API 1 and adapter protocol 5 remain unchanged. Existing provider owners and the production Herdr service are retained.

## Validation

Rust tests, Release lint/compilation and guarded Android instrumentation are run before publishing. The final public-package tests require two new sessions, one bounded model reply, same-session reconnect with retained history, no terminal auto-display or false sync error, three actual 1080p stream cycles, and an explicit failure after stopping only the isolated Sunshine fixture. Publication, deployment and cleanup results are recorded after completion.

Disabling audio capture was first validated in three isolated 1080p stream cycles and temporarily applied between production sessions, retaining Herdr and Kite Node. After the user requested audio, that temporary workaround is being replaced with actual playback and host-device preservation. The final evidence must record restored production capture, decoded non-silent test audio, AudioTrack playback, focus/mute behavior and repeated disconnects. Those final results are attached below. See the [installed Sunshine source](https://github.com/LizardByte/Sunshine/blob/v2026.914.233613/src/audio.cpp#L146-L151) and its [configuration](https://docs.lizardbyte.dev/projects/sunshine/v2026.319.132152/md_docs_2configuration.html#stream_audio).

The official [Codex App Server documentation](https://learn.chatgpt.com/docs/app-server#threads) was checked against the owned live thread. This release does not replace the provider, create a second owner or change the provider protocol.

## Alpha 24 audio throughput finding

The public Alpha 24 package passed new-session/first-reply/reconnect checks and audio liveness, menu mute and focus checks. A first accessibility lookup returned no menu node; an unchanged rerun completed the full audio test. Further sustained-playback measurement nevertheless failed: after warmup, four seconds received 802 packets, discarded 450, and advanced playback by only 82,560 frames (42.9% of real time), with 109 underruns. [Measured baseline](audio-alpha24-baseline.json). Liveness alone was insufficient acceptance.

Alpha 25 requests the pinned Moonlight transport's 10ms Opus mode for the MediaCodec receiver, batches codec input/output and reduces playback-counter polling. Its audio test now measures a warmed four-second interval and requires at least 95% real-time playback with at most 2% application queue drops, alongside mute/focus/resource and video checks. Final Alpha 26 package acceptance is recorded below.

## Session-operation ordering

The public Alpha 25 audio test met the steady-playback target: 98.0% real-time playback, one queue drop in 400 packets (0.25%), and zero steady-window underruns. Its latest new-session test completed the first reply and same-session reconnect, then attempted another create while `openSession` still held the native busy guard. The command was ignored. UI new-session entries and confirmation were also enabled during that guard, so Alpha 26 disables them consistently and the test waits for completion before creating the next session. Audio production code remains unchanged from the passing Alpha 25 implementation.

## Final public Alpha 26 acceptance

Public [APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.26/kite-0.2.0-alpha.26-arm64-v8a.apk), versionCode 28, source `ad1ecfda2797a290144df52de1bfd523ec282448`: 21,672,379 bytes, SHA-256 `c778618162661a8f19d897b82dcbf43656bba6b87666dd5cba9e39c13c836f4c`. Both main/tag CI passed 95 Rust tests, Android Release JVM/lint and host checks. Independently verified public bytes, stable v1/v2/v3 certificate and 16KiB alignment pass. In-place installation retained all 28 recorded private files before launch, including four profiles, identity, trust and current pairing. [Publication](published-release.json), [upgrade](android-upgrade.json).

The exact public APK passed all four guarded Android API 36 ARM64 tests. They target separate owned SSH/Sunshine ports, an Xvfb moving scene and a private PulseAudio server with a synthetic tone; no user's desktop or audio route is used.

| Area | Final result |
| --- | --- |
| New sessions | Two fresh sessions ready; first marker reply received; same session/history restored after reconnect; no automatic terminal, false sync error or cross-session text |
| Audio format | 48kHz stereo, 480 samples / 10ms packet |
| Warmed audio, 4.0002 seconds | 399 packets, 0 application queue drops, 190,304 played frames / **99.1%** of real time, 1 underrun |
| Audio interaction/lifetime | Three stream cycles, real non-silent decoded PCM and AudioTrack progress; menu mute/resume, transient focus loss/return and worker release pass |
| Video with sound enabled | Source and stream **1920×1080**; 60fps requested, **53.39fps actually rendered**, 55.88fps received over 12.04 seconds |
| UI/decoder timing | UI-frame P95 16.67ms; queue/render/callback P95 7.8/11.4/2.1ms; no input-buffer timeouts, rejections or explicit decoder IDR requests |
| Desktop recovery | Three actual video reopen/stop cycles; idle has an action, not a spinner; deliberate owned-server termination becomes explicit failure while SSH remains connected |
| Owned Sunshine | Eight connected streams in the final server invocation, no PulseAudio assertion and no default audio-sink switch before intentional interruption |

[Session results](new-session.json), [audio results](audio.json), [video results](video.json), [recovery](recovery.json), [server counts](sunshine-audio-validation.json). These counters demonstrate the emulator pipeline and controlled local link, not audible physical speakers/Bluetooth or mobile Tailscale A/V latency. Audio output is not recorded. The same-app transient focus request is a controlled focus-lifecycle check, not a real phone-call test. The roughly 53.4fps video measurement is close to the earlier 54.6fps local baseline; it does not claim measured 60fps or prove the earlier WAN video issue is eliminated.

## Production host and cleanup

Production Sunshine's temporary audio-disable line was removed and the original configuration bytes restored between streams. Only Sunshine was restarted for that restoration; Herdr and Kite Node stayed up. Capture is enabled by its original default. New clients keep local host playback and avoid requesting a default-device swap. This remains a mitigation, not a patched PulseAudio library. [Restoration](audio-restoration.json).

The matching official Linux Alpha 26 archive was installed with the same node identity and current Herdr/Sunshine processes. Eleven stable private files are byte-identical; the live session catalogue legitimately changed during QA session creation, so it was checked semantically for all 13 records rather than compared with a stale pre-test hash. The native status panel/tray opens and hides without stopping the node; health is `ready`. [Deployment](linux-deployment.json).

Cleanup stopped seven precisely owned provider groups/workspaces, removed their catalogue/registry entries and deleted only three owned persisted test threads. Other sessions and provider owners were retained. The isolated Sunshine, private PulseAudio/D-Bus group, tone generator, scene/display, guest/Mac relays, three ADB reverses, test APK and emulator were stopped or removed. The original QA pairing, rotation and authorized-key bytes were restored. The public app/data remain installed in that AVD. Five pre-existing dirty primary-checkout files are unchanged. [Local cleanup](local-cleanup.json), [host cleanup](remote-cleanup.json).

Physical-phone Tailscale stability, speaker/Bluetooth audibility and end-to-end A/V synchronization remain to be confirmed on the user's device. The 09:32 short disconnect still has no established single cause; the fixes and bounded tests above must not be presented as proof of that network path.
