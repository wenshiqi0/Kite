# Compact chat image and work-result layout

Tool results that contain both a tool block and media use the same full work-card
width as neighboring tool groups. They no longer nest a second padded chat
bubble around the tool card. Normal human/agent bubbles retain their prior
alignment, backgrounds and distinct padding.

Images use rounded full-width thumbnails. Tall images have a bounded 240 dp
preview, showing the top of long screenshots with a full-image hint; opening the
preview still displays the original complete image. Large filename buttons are
removed from the chat flow; the filename remains available in the full preview.
Consecutive attachments are batched in source order, with blank Markdown
separators joining image albums. Two/four-image albums use two columns and other
albums use three. Text/tool boundaries and non-image attachment order remain.

Validation:

- 87 Android Release JVM tests, compilation and Release lint passed. Grouping
  regressions preserve meaningful text/tool boundaries and whitespace albums.
- Three Android 16 native tests passed: mixed tool/media and album layout,
  shared content inside distinct sender bubbles, and audio/video/text previews.
- Native assertions verify equal pure/mixed tool-card width, a full-width bounded
  long thumbnail, square aligned gallery tiles, hidden thumbnail filenames and
  visibility of the original image's bottom after opening the full preview.
- The previous copy-button removal and H.264/AAC media sample remain included.

All native data is synthetic. No user screenshot is committed as a fixture.
Published [Alpha 101](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.101)
/ versionCode 103 from `601abf7701012819ef99cc0e216fff55762f8d34`.
Both [tagged CI jobs](https://github.com/wenshiqi0/Kite/actions/runs/43049125593)
passed. Independent downloaded-byte checks matched every SHA256SUMS entry and
the established signing certificate. SDK checks verified primary v1/v2/v3 and
compatibility v1/v2 signatures, matching application payloads and 16 KiB ZIP
alignment. [Public artifact metadata](published-release.json) records the assets.
