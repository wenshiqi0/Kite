# Android Alpha 6 acceptance — 2026-10-30

The [archived product designs](../2026-10-29/README.md) remain the interaction reference. These are captures of the native app on an Android 16 / API 36 ARM64 emulator with a real Linux X11 host, herdr 0.9.5, Codex 0.164.0 and Sunshine. They show controlled validation conversations, not the user's private chat.

## Product checks

- Watch / Operate uses a single mode entry. Common keyboard controls are grouped; zoom, input mode and secondary tools have separate compact entries. Agent pause remains independent. [Watch](watch.png), [Operate](operate.png).
- Operate can zoom the source desktop locally; the source remains 1920×1080. Existing viewport inverse-coordinate tests pass. [150% zoom](operate-zoom.png).
- A left-edge swipe does not open navigation; clicking the menu still opens it.
- Native system pickers selected PNG, text and WAV files. Three verified uploads appeared in the draft, and the sent history retained file cards without transport markup. [Composer](composer-attachments.png), [Sent message](chat-sent.png).
- The phone selected GPT-6-Luna and medium reasoning, then received exactly `KITE-ANDROID-RICH-OK`. The corresponding host `turn_context` recorded `model=gpt-6-luna`, `effort=medium`; the user input contained one native image plus metadata for all three uploaded files (431, 15 and 1644 bytes).
- A separate Rust integration probe used GPT-6.1-Sol / low. The agent identified the cyan image, read the exact text fixture and reported the audio file's size. Backend process pause / resume was verified without pausing on entry to Operate.
- A real terminal approval was declined through the phone's confirmation card; the card cleared without an error. The RPC client does not take approval ownership from the attached TUI.
- The production pane retained its original terminal and Codex thread UUID during migration; the global Codex app-server and Sunshine remained running. No test prompts were submitted to that user conversation.

## Native regression

File-picker return exposed an ART abort: `JNI DETECTED ERROR ... a thread is making JNI calls without being attached`, in `AndroidSink::supports`. JNA had detached a Tokio worker while jni-rs 0.21 retained its TLS guard. Reattaching replaced that guard, whose destructor detached the new environment. `AndroidSink::env` now clears a stale guard while the VM reports the thread detached, before attaching again. Sink field destruction repairs the attachment before releasing GlobalRefs too. Eight consecutive file-picker open/cancel/return cycles passed in the same process with a clean ART crash buffer after the fix.

Large image records are projected on the host before transport: inline image bytes are omitted while text and attachment manifests remain intact. A regression fixture larger than 3 MiB verifies that image data does not crowd the user message out of the bounded history window; the original provider file is unchanged.

## Build checks and limits

Rust workspace tests, Clippy with warnings denied, Android lint and seven viewport unit tests, three host-adapter tests, three release-publisher tests and iOS ARM64 simulator compilation passed. Tagged CI must independently build and verify the signed Release APK before publishing the [Alpha prerelease](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.6).

The current host catalog advertises text and image, not native audio. WAV is therefore uploaded as a file for tools to read; no transcription or native hearing is claimed. Rich composer settings/uploads currently require Codex. Claude terminal chat remains text only. Codex app-server transport remains experimental. See [host setup](../../backend-setup.md) for deployment and upgrade details.
