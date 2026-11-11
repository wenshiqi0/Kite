# Multimodal Items and shared author presentation

The same Item/Block contract carries human, agent and tool content. Android uses
one Markdown renderer, card layout, copy action and attachment component. Authors
are labelled without changing content capabilities, alignment or colours.

The host preserves text/media order from Codex, Claude, DSH and Ply, including
media-only ACP replies and media in tool results. Native inline bytes and existing
Markdown image/file references become immutable attachment descriptors; the
provider's original history is never rewritten. Network resources download only
when viewed, with a 25 MiB bound, transport checksums and local caching. Existing
uploaded images retain their envelope identity instead of appearing twice.

Android images display inline and open the shared zoom/save preview. Audio and
video open native playback controls without autoplay. Text files have a bounded
preview; other files can be saved. The macOS client receives the same attachment
blocks and uses the existing image/save controls; no macOS execution is claimed.

Regression coverage includes stable record identity across presentation changes,
source-order pagination, cache round trips, optimistic input acknowledgement,
native ACP chunks, media-only messages, legacy attachment envelopes, local files
and lazy remote downloads. A generated two-second video fixture and synthetic
PCM audio are used only by native tests.

Validation on 2026-11-11:

- Bun type, boundary and lint checks: passed; 110 tests passed, one Darwin-only
  case skipped on Linux.
- Rust `kite-agent`, `kite-app` and `kite-ffi`: 193 tests passed, one opt-in real
  transcript case ignored; clippy with `-D warnings` and formatting passed.
- Android: 85 Release JVM tests passed; Release lint and native test compilation
  passed.
- Three native Android 16 emulator tests passed against the owned offline fixture:
  `humanAndAgentShareMarkdownImageLayoutAndPreviewControls`,
  `nativeAudioVideoAndTextAttachmentsOpenWithoutAutoplay` and
  `SessionImagesPresentationTest.statusBadgesAndOfflineGeneratedImagePreviewRender`.
  Captures verify equal Markdown colours, sizes and image bounds for both authors,
  both zoom/save previews, actual WAV and video playback, non-autoplay and text
  attachment rendering. The video fixture exposed and verified the fix for a
  player being stopped by the initial Compose state transition.

The emulator ran in the existing isolated QA container with a new disposable AVD;
no real host or saved user session was connected. Native sources use the shared
arm64 core through Android's translation layer. No physical-handset,
authenticated remote-media or real-model generation result is inferred.

Alpha 94 / versionCode 96 ships the Android and Linux host packages together via
the tagged release workflow. Install the matching host to enable the new history
projection. The macOS edit is a source-level author-label colour adjustment only.
Published [Alpha 94](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.94)
from `99728529756798ff5dcaf96d9741cc2882c0f24d`.
Both jobs in [tagged CI](https://github.com/wenshiqi0/Kite/actions/runs/42998393979)
passed. Independent downloads matched every public SHA256SUMS entry; SDK checks
confirmed the Alpha 93 certificate pin, primary v1/v2/v3 and compatibility v1/v2
signatures, equal application payloads and 16 KiB ZIP alignment. Full public
metadata is in [published-release.json](published-release.json).
