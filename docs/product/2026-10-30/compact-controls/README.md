# Compact desktop controls — design proposal

**Status: interactive design only. No Android client change or new APK is delivered by this proposal.** The user reviewed Alpha 11 and reported excessive control size, a bulky collapsed state, restricted dragging and poor space usage in portrait split view.

The user's follow-up chose a specific portrait interaction: open a separate bottom surface with a laptop-style trackpad above left/right buttons and a wheel between them. Closing it returns the space to chat. Fullscreen uses a separate compact mouse control informed by the user's UU expanded/collapsed reference images. Those images were reviewed as references; the captures below use only constructed test content.

The complete decisions and native acceptance requirements are in [RFC 0011](../../../rfcs/0011-compact-desktop-controls.md). Existing source resolution, input coordinate mapping, explicit Watch/Operate, separate agent pause, history/identity and automatic clipboard behavior are retained.

## Preview states

- [Bottom trackpad open](split-open.png): chat and composer participate in layout above a 176dp bottom surface; video remains clear.
- [Bottom trackpad closed](split-closed.png): the surface disappears and space returns to chat; Operate remains selected.
- [Small window](small-window.png): a 320×640 logical window uses a 144dp trackpad and retains a usable chat region.
- [Fullscreen collapsed mouse](fullscreen-icon.png) and [expanded mouse](fullscreen-mouse.png): small visible graphics, explicit touch areas and workspace-level positioning.
- [Dark appearance](split-dark.png): the same geometry at a narrow preview width.

[Interactive preview source](compact-controls.html) is a self-contained fragment for the conversation preview runtime. Its controls simulate cursor movement, buttons, wheel, panel/keyboard exclusion and fullscreen dragging locally. It does not connect to SSH, Sunshine, a model, system clipboard or a native decoder. The keyboard region represents a keyboard surface; it is not a custom keyboard implementation.

Browser checks are recorded in [preview-checks.json](preview-checks.json). They verify the design model only, not Android gesture delivery, device responsiveness or backend behavior. The previous [Alpha 11 acceptance](../hybrid-input/acceptance.md) remains the shipped implementation record until this proposal is implemented.
