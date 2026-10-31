# Documentation

- [Host setup](backend-setup.md): SSH, herdr, Sunshine, pairing and troubleshooting.
- [Linux desktop host](linux-desktop.md): install Kite Node and the native control panel, reuse services and expose MCP queries.
- [Implementation status and next stages](implementation-plan.md): delivered Android features, herdr-core evaluation, desktop nodes and multi-machine MCP.
- [Development](development.md): source layout, Android / iOS builds and generated bindings.
- [Testing](testing.md): offline checks, integration fixtures and acceptance boundaries.
- [Alpha releases](releases.md): CI, stable signing, version tags and GitHub prereleases.
- [Product reference](product/2026-10-29/README.md): original chat / desktop mockups and confirmed interaction rules.
- [Engineering specification](../SPEC.md): architecture and ownership rules.
- [RFCs](rfcs/README.md): design decisions and implementation scope.

## Delivery and evaluation records

- [Anchored fullscreen input and Linux node](product/2026-10-31/anchored-input-and-linux-node/acceptance.md): fixed control-relative hotspot, real quiet-window timing, persistent host/panel/MCP and owned provider chat.
- [Attachment-only composer menu](product/2026-10-31/attachments-only-menu/acceptance.md): three resource entries, retained desktop navigation, current skill-picker support and public APK verification.
- [Compact mouse, tactile wheel and simple host cards](product/2026-10-31/compact-mouse-and-hosts/acceptance.md): smaller controls, bidirectional wheel taps, actual system haptic dispatch and connection-only card presentation.
- [Fullscreen virtual pointer](product/2026-10-31/virtual-pointer/acceptance.md): explicit arrow, source-coordinate mouse buttons/wheel, held dragging and real host event comparisons.
- [Complete transcript and dark workspace](product/2026-10-31/complete-transcript/acceptance.md): 10,000-record data/cache checks, real native view counts, independent scrolling and mouse-reference fullscreen.

- [Alpha 12 native controls and history](product/2026-10-31/compact-controls/acceptance.md): collapsible portrait trackpad, compact fullscreen mouse, direct system IME, host cards and paged virtual history; includes published APK verification and retention checks.
- [Accepted compact controls prototype](product/2026-10-30/compact-controls/README.md): archived interactive design reference; native evidence is recorded above.
- [Alpha 11 interaction acceptance](product/2026-10-30/hybrid-input/acceptance.md): automatic text clipboard, hybrid mouse/touch, rich text and sidebar fixes; includes public APK verification and upgrade results.
- [herdr-core restoration evaluation](product/2026-10-30/herdr-core-migration.md): same-thread recovery with the existing provider and after a fresh provider process restart; remaining migration requirements.
- [Alpha 10 installation investigation](product/2026-10-30/install-signatures/acceptance.md): signing checks, emulator update and subsequent user-reported handset installation success.
- [Earlier product acceptance](product/2026-10-30/acceptance.md): Alpha 6 model settings, attachments, approvals and native regression results. Later records are linked from [Testing](testing.md).
