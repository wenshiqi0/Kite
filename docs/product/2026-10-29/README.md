# Chat and desktop product reference

These are the three original product mockups supplied by the user on 2026-10-29. The PNG files are preserved unchanged; `assets.json` records their SHA-256 digests.

1. [Chat with a draggable desktop window and system PiP](kite_chat_floating_desktop_mockup.png)
2. [Fullscreen viewing and takeover](kite_fullscreen_watch_vs_takeover.png)
3. [Portrait split view and landscape desktop](kite_split_and_fullscreen_desktop_mockup.png)

## Confirmed behavior

- Chat is the main workspace, with a visible composer, agent identity and activity status.
- The desktop can float above chat, dock to an edge, share the screen vertically, or open fullscreen.
- Fullscreen defaults to watching; watch gestures zoom and pan locally and do not send remote input.
- Takeover is explicit. **It does not automatically pause the agent.** Pausing is a separate action. This decision supersedes the paused-agent caption in the third mockup; the user confirmed it explicitly after reviewing the conflict.
- Leaving the app while watching a desktop uses Android system picture-in-picture.
- Message, tool and approval cards must represent real herdr / agent data and real supported actions. The sample shell and database text in the drawings is illustrative content, not an instruction to execute it.
- Preserve source desktop resolution with proportional phone fitting, allowing modest downsampling for interaction performance.
- Preserve the user's existing device key, saved hosts, trust records and pairing identity when upgrading.

Android Alpha 2 implements these flows. See the [acceptance record and actual captures](acceptance.md), and [RFC 0004](../../rfcs/0004-chat-desktop-workspace.md) for the implementation boundary.

The phone feedback follow-up uses **Watch / Operate**, with source-pixel input mapping and an explicit session entry. See [Alpha 5 verification](phone-fixes.md).
