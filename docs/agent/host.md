# Host health

Linux and macOS expose the same Node, session and MCP control protocol. Mac nodes use launchd and report `resourceApi=0`: Linux kernel memory budgets, process pause and X11 desktop MCP actions are unavailable there. Sunshine streaming remains separate. Mac setup and service controls: `kite-node host inspect|install|enable|update|disable`; local private logs live under `~/.local/lib/kite/macos/logs`.

`kite resources` reads CPU, available memory, Swap, pressure, top resident processes and the shared agent budget. `limit_enforced` is a kernel-verified fact; missing or stale data does not mean the host is healthy. The Android host detail exposes the same facts and budget editor.

On Linux, the local default budget is 10 GiB for actual memory used by managed agent runtimes and native child processes together. Displayed usage, warnings, startup admission and budget edits all exclude reclaimable unmapped file cache and reclaimable slab; resident file mappings, tmpfs and pending file writes remain charged. Cache is reported separately. `kite resource-limit --bytes 10737418240` applies and persists a requested budget. A shrink below actual use/reservations is rejected; a safe shrink reclaims cache before changing the kernel ceiling. New agent starts are paused under critical memory pressure or when quota enforcement is unavailable; existing work and history access remain available. Independent containers and external services are visible in host totals separately.

Linux's hard ceiling includes cache. Kite keeps that ceiling within the requested budget (less migration reservations); the kernel reclaims cache as resident allocations grow. It never raises the ceiling by the cache amount, so reclaiming cache cannot allow actual memory to exceed the requested budget.

`kite status` reads the Node version, identity and configured service health. `ready` means those local services are active; it does not prove that a phone is connected, a model turn succeeds or desktop media is decoded.

`kite status --refresh` forces service inspection. `kite ensure all` starts configured inactive services; it preserves active external services. Closing the native panel affects only the GUI. Existing agent owners have their own lifetime.

Desktop availability is checked separately with `kite desktop status`. Docs are readable offline, so a documented feature may still be unavailable on this host. Service diagnostics: `systemctl --user status kite-node.service kite-node.socket` and the matching user journal. Never include keys, provider tokens or message bodies in reports.
