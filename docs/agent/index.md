# Kite agent docs

Kite provides local host queries and configured Linux X11 desktop control through CLI or standard MCP. Read one task guide directly:

- `kite docs desktop` — status, screenshots, mouse, text, keys and app launch.
- `kite docs desktop/input` — precise coordinates, regions and input limits.
- `kite docs host sessions` — service health and saved references.
- `kite docs harnesses` — shared skill and MCP setup for any compatible harness.
- `kite docs collaboration` — authenticated ACP peers, remote sessions and explicit task delegation.
- `kite docs upgrades` — live handover and maintenance boundaries.

`kite docs --json` lists topic revisions. Reuse a retained body with `kite docs desktop --known desktop=REV`; unchanged bodies are omitted. Missing context needs a fresh body. Docs describe interfaces; `kite desktop status` checks actual availability. Local commands target this Node's configured desktop; SSH can run the same CLI on another authenticated host.
