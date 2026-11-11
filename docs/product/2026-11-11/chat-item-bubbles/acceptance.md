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

Publication and independent public artifact verification are recorded below.
