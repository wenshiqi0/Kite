# herdr-core restoration experiment — 2026-10-30

The user selected evaluation of herdr-core migration with history, trust and device identity retained. This experiment uses an isolated Linux runtime and a disposable Codex conversation. The production Herdr TUI service and user thread are left running.

## Verified

- Downloaded herdr-core **0.1.14** Linux x86_64 from the official HTTPS release manifest; archive SHA-256 `e9dbabdead0ce0ce3de51b6f54cc9549efedbb41b2f8448b6932ab0d1442343e` matches. It runs with a separate `HC_SESSION`, socket and scratch XDG state/config/runtime directories.
- Started a validation Codex thread in an owned Herdr TUI workspace against one private app-server. The first turn stored a known conversation token and returned `KITE-MIGRATION-OLD-READY`.
- Closed only that validation TUI workspace. Its detached provider owner remained alive.
- Started a new `hc` terminal using `codex --remote <same private owner> resume <same thread>`. The new terminal displayed the original message and reply. `hc get` reported a live Codex terminal with idle program status.
- Sent a follow-up through **hc's terminal API**, not by creating another provider thread. The actual model returned `KITE-MIGRATION-RESTORED CEDAR42`, proving the previous context was retained. The original provider thread remained loaded in the same app-server.

The proof is terminal transport migration plus preserved provider history/context. It is not a full production rollout or a reboot-recovery claim.

## Remaining migration requirements

The old private app-server still inherited the legacy pane environment. Reattaching a TUI does not rewrite the environment used by model tools. A product migration must bind runtime-independent agent/provider identities, route messages and approvals by that identity, and define a clean provider-owner transition at an idle or explicitly approved boundary. It must not launch a second owner for an already running thread.

Herdr-core terminals use addresses such as `t1`, distinct from Herdr TUI pane/terminal IDs. The new backend needs an explicit adapter and mapping; changing the executable name is insufficient. Questions, execution approvals, archive/restore, connection loss, parent/child lifecycle and provider-owner restart need separate validation before switching production.

Sources: [herdr-core](https://herdr.dev/core/), [separation from Herdr TUI](https://herdr.dev/#questions), [runtime security and sessions](https://github.com/herdrdev/herdr-core/blob/master/docs/security.md).
