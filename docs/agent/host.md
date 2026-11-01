# Host health

`kite status` reads the Node version, identity and configured service health. `ready` means those local services are active; it does not prove that a phone is connected, a model turn succeeds or desktop media is decoded.

`kite status --refresh` forces service inspection. `kite ensure all` starts configured inactive services; it preserves active external services. Closing the native panel affects only the GUI. Existing agent owners have their own lifetime.

Desktop availability is checked separately with `kite desktop status`. Docs are readable offline, so a documented feature may still be unavailable on this host. Service diagnostics: `systemctl --user status kite-node.service kite-node.socket` and the matching user journal. Never include keys, provider tokens or message bodies in reports.
