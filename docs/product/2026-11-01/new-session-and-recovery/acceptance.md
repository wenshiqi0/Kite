# New sessions, desktop audio and recovery — Alpha 25

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

Disabling audio capture was first validated in three isolated 1080p stream cycles and temporarily applied between production sessions, retaining Herdr and Kite Node. After the user requested audio, that temporary workaround is being replaced with actual playback and host-device preservation. The final evidence must record restored production capture, decoded non-silent test audio, AudioTrack playback, focus/mute behavior and repeated disconnects. Until those results are attached, audio acceptance is pending. See the [installed Sunshine source](https://github.com/LizardByte/Sunshine/blob/v2026.914.233613/src/audio.cpp#L146-L151) and its [configuration](https://docs.lizardbyte.dev/projects/sunshine/v2026.319.132152/md_docs_2configuration.html#stream_audio).

The official [Codex App Server documentation](https://learn.chatgpt.com/docs/app-server#threads) was checked against the owned live thread. This release does not replace the provider, create a second owner or change the provider protocol.

## Alpha 24 audio throughput finding

The public Alpha 24 package passed new-session/first-reply/reconnect checks and audio liveness, menu mute and focus checks. A first accessibility lookup returned no menu node; an unchanged rerun completed the full audio test. Further sustained-playback measurement nevertheless failed: after warmup, four seconds received 802 packets, discarded 450, and advanced playback by only 82,560 frames (42.9% of real time), with 109 underruns. [Measured baseline](audio-alpha24-baseline.json). Liveness alone was insufficient acceptance.

Alpha 25 requests the pinned Moonlight transport's 10ms Opus mode for the MediaCodec receiver, batches codec input/output and reduces playback-counter polling. Its audio test now measures a warmed four-second interval and requires at least 95% real-time playback with at most 2% application queue drops, alongside mute/focus/resource and video checks. Final package acceptance remains pending until these pass.
