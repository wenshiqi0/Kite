# Node upgrades

Compatible live upgrades preserve accepted connections in their original generation and route new connections to a prepared replacement. A short-lived launcher lets the user manager adopt the replacement before MAINPID changes; old generations drain naturally. Neither messages nor approvals are replayed.

Use the matching host installer's `--enable` flow. A running pre-handover Node requires an explicit first maintenance migration, `--migrate-node`; that one-time switch reconnects clients. Ordinary service restart, host reboot, crashes and protocol/schema migrations are separate from a live upgrade.

`kite status` confirms the current identity/version/health, not complete application acceptance. Candidate binaries require Node API 1, backend protocol 5 and generation-launch support. Keep the user's config, trust and live session data; never restore an old catalogue over concurrent progress. GUI updates wait for a hidden idle panel, or normal exit/next launch without a tray.
