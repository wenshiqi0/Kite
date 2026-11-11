# Video and Markdown attachment preview

The reported video could open to a black screen without an obvious play control, and its attachment row used the generic file icon. The previous native test started the underlying `VideoView` directly, so it did not verify that a user could start playback.

The preview now uses a texture-backed player in the dialog window, a poster frame, persistent Compose play/pause and seek controls, and explicit error/retry UI. Opening the preview does not autoplay. Closing it or leaving the foreground releases or pauses playback. Attachment icons use both MIME and filename, including a distinct video mark.

The reported `AGENTS.md` loading error was reproduced with the installed host's `attachment-read` command. Its registered source was a GitHub `/blob/…/AGENTS.md` web page, which the media projector incorrectly promoted to a downloadable attachment based solely on `.md`. Ordinary HTTP Markdown links now remain navigable links. Local Markdown files and explicit remote file resource blocks retain attachment support. Both native and legacy history stamp versions change so existing history is reprojected.

## Validation

- Android Release JVM tests: 87 passed; Release lint passed.
- Native Android emulator: `nativeAudioVideoAndTextAttachmentsOpenWithoutAutoplay` and `toolMediaUsesFullWidthAndImagesFormCompactAlbums` passed. The video test waits for the enabled visible play button and its duration, clicks that button, confirms the pause button and advancing progress, then pauses it. The same test opens audio and a UTF-8 Markdown file containing Chinese text.
- TypeScript typecheck, Biome and boundary checks passed; 116 package tests passed, one Darwin-only test skipped. Regression coverage verifies GitHub and ordinary HTTP Markdown links stay links, local Markdown bytes remain readable, and explicit remote Markdown downloads are fetched lazily and cached.
- Rust history tests: 22 passed.
- The installed host successfully transported a normal 109-byte Markdown sample unchanged; the problematic GitHub page source reproduced `attachment download failed` with no response bytes.

The emulator uses owned fixtures without user profiles or credentials. Physical-phone playback and installation remain user acceptance. Screenshots show [playing video](video-playing.png) and [opened Markdown](markdown-open.png).

The previous user/assistant bubble styles, image album layout and selectable text remain in place.

## Published delivery

[Alpha 102](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.102), Android versionCode 104, was published from `5de9b425cb0821e0feef9cfd515cd54eeac0a93e`. Tagged CI passed on attempt two: the first attempt exceeded the existing five-second native Linux screenshot wait, and the unchanged source passed on rerun. Independent downloads verified all checksums, the stable certificate, primary v1/v2/v3 and compatibility v1/v2 signatures, equal application payloads and 16 KiB alignment.

Both wen-h255 and wen-mini now run the matching public host package. Their 111 and 8 existing session references, node identities, configuration/trust hashes and external Herdr/Sunshine PIDs were preserved. The installed host also retained the Markdown web link and original message identity while transporting the local Markdown file unchanged, without sending a model turn. Exact public asset and host evidence is in [published-release.json](published-release.json).
