# Shared Item renderer with distinct sender bubbles

The user's clarification narrows the shared rendering contract to the content
inside the bubble. Alpha 94 incorrectly also changed the outer bubble layout.

Alpha 95 restores the previous right-aligned human / left-aligned agent layout,
sender backgrounds and 340 dp maximum bubble width. Human bubbles use 12/10 dp
horizontal/vertical padding; agent bubbles use 15/11 dp. The same `ItemContent`
component still renders both authors' Markdown, images, audio, video and files.
The added author headings are removed. The macOS source-label colour change is
also reverted. Provider parsing, media transport and attachment previews retain
the verified Alpha 94 implementation.

Validation on 2026-11-11:

- 85 Android Release JVM tests and Release lint passed; debug app and native test
  sources compiled successfully.
- Two Android 16 native tests passed in a new isolated AVD:
  `humanAndAgentShareContentRendererInsideDistinctBubbles` and
  `subagentHistorySharesGlobalInstructionsSkillCardsAndMessageBubblesWithMainChat`.
  The first checks right/left geometry, the actual user/agent background pixels,
  distinct padding, equal native Markdown style and both shared image previews.
  The second verifies that main chat and read-only related-task history continue
  using the same complete row component.
- The existing Alpha 94 provider, transport and media validation remains valid:
  those implementations are unchanged. No physical-handset result is claimed.

Published [Alpha 95](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.95)
/ versionCode 97 from `41323c26b4148564468df7b7addc8c65ad44fc9c`.
Both [tagged CI jobs](https://github.com/wenshiqi0/Kite/actions/runs/43006741757)
passed. Independent downloads matched the published checksums and the Alpha 94
certificate pin; SDK checks verified primary v1/v2/v3 and compatibility v1/v2
signatures, identical application payloads and 16 KiB ZIP alignment.
[Public verification metadata](published-release.json) records the exact assets.
Users already on the Alpha 94 host only need to update the Android client for
this visual correction.
