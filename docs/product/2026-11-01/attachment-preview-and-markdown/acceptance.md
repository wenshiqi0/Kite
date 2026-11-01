# Attachment preview and Chinese Markdown boundaries

The screenshot's source message was checked locally. Its strong span ended with a Chinese sentence mark immediately followed by `**Kite`. CommonMark treats that boundary as an opener, leaving literal markers. A minimal synthetic `**已经完成。**Kite` reproduces the literal output; a closing boundary space produces a strong span. The user's screenshot and conversation content are not copied into this repository.

`normalizeCjkStrong` repairs this boundary only for presentation. Shared message storage and copy-source values remain unchanged. Fenced/indented code, inline backticks, escaped markers and link destinations are protected. JVM cases cover the boundary and literal protections; the native span regression checks real Markwon strong spans and literal code.

Uploaded image/audio/text rows now open native previews, including composer drafts. Images support zoom/pan; audio prepares without autoplay and supports play/pause/seeking, focus handling and stopping on close/background. Text previews support UTF-8 and UTF-16 BOM, are bounded to 128 Ki characters and remain selectable; original full files can be saved. Opening previews pauses chat following. Generated-image preview retains its existing presentation and now also pauses following. Draft local copies remain until successful send/removal; completed drafts can fall back to the scoped remote file when a cache copy is missing.

The new read-only `attachment-read` route accepts an upload ID and validated basename. It opens only the private upload directory, rejects directory/file symlinks, nonregular files and over-limit files, and returns a checksum/size/name header plus binary data. Rust validates conversation/draft membership, checks transfer integrity and rejects a changed context. Private native caches verify their own stored checksums. Both clients and the matching host need the new interface; no arbitrary remote pathname is exposed to the native preview.

## Retained fixes and build gate

This source includes the question-wait and 240/96 dp verified-tail navigation fixes from the unpublished Alpha 34 candidate. That tag is retained unchanged after its Linux integration probe failed by reading an empty heartbeat during `write_text` truncation. The owned heartbeat fixture now uses atomic replacement. The controlled real-systemd upgrade probe passes locally; it does not operate production service names.

Concurrent main-branch agent documentation/desktop changes are incorporated. Alpha 35 belongs to that source update; the combined Android release advances to Alpha 36 without moving earlier tags.

## Validation scope

Rust tests include real authenticated SSH attachment framing and rejection of changed name/size/checksum. Python tests cover file framing, multiple files in an upload directory, name/ID traversal, symlinks and nonblocking FIFO rejection. JVM tests cover Markdown boundaries, protected literals, preview types, time labels, BOM decoding and bounded text. Native span/presentation sources compile in CI; phone display, playback, gestures and system Documents export remain a separate execution boundary.

## Combined check results

Local checks pass: 125 Rust tests, Clippy, 33 backend tests, 8 publisher tests and 5 installer tests. The isolated real-systemd upgrade probe passes with atomic heartbeat writes. The new main-branch desktop stress check exposed dropped Unicode input on the owned GTK target; non-ASCII dispatch uses 24 ms pacing and fixture observations avoid synchronous per-key geometry/file I/O. One private Xvfb run passed, but an independent confirmation run still lost a character in the short sample. The 24 ms candidate is therefore not accepted as a complete input fix; validation remains open. The Android preview/Markdown source passed the earlier PR compilation and JVM cases; the final retained-draft changes are also checked by the trusted release build.
