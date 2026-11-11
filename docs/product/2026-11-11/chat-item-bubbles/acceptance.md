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

The native regression checks different sender alignment, background pixels and
padding while requiring identical native Markdown rendering and shared image
preview/save controls. Results and publication verification are recorded below.
