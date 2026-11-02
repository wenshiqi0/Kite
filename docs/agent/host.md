# Host health

`kite resources` reads CPU, available memory, Swap, pressure, top resident processes and the shared agent budget. `limit_enforced` is a kernel-verified fact; missing or stale data does not mean the host is healthy. The Android host detail exposes the same facts and budget editor.

The local default budget is 10 GiB for managed agent runtimes and native child processes together. `kite resource-limit --bytes 10737418240` applies and persists a requested budget. A shrink below current use/reservations is rejected. New agent starts are paused under critical memory pressure or when quota enforcement is unavailable; existing work and history access remain available. Independent containers and external services are visible in host totals separately.

`kite status` reads the Node version, identity and configured service health. `ready` means those local services are active; it does not prove that a phone is connected, a model turn succeeds or desktop media is decoded.

`kite status --refresh` forces service inspection. `kite ensure all` starts configured inactive services; it preserves active external services. Closing the native panel affects only the GUI. Existing agent owners have their own lifetime.

Desktop availability is checked separately with `kite desktop status`. Docs are readable offline, so a documented feature may still be unavailable on this host. Service diagnostics: `systemctl --user status kite-node.service kite-node.socket` and the matching user journal. Never include keys, provider tokens or message bodies in reports.
