# Linux taskbar status acceptance

The Linux host now has a resident notification-area icon. A green check, amber warning, red cross and clock badge communicate ready, degraded, unreachable and processing states. The menu and tooltip show Node and individual service states. Rust computes aggregate configured-service health; the native GTK UI presents it and clears old healthy content when Node disconnects or a status command times out.

The panel and tray share a three-second poll. Left-click opens the existing window; right-click opens status and actions. Closing the panel returns to a usable tray, and exiting the status UI leaves the node, external services and agents running. Desktop-login autostart uses the stable installed launcher and preserves existing user opt-out settings.

## Native checks

An isolated CLI fixture drove real GTK widgets on wen.linux's Xfce/X11 desktop. [The recorded check](xfce-validation.json) confirms the tray was embedded, its popup opened, service degradation changed the UI, a disconnect cleared stale healthy rows, a slow command timed out, service state recovered, repeated background and normal launches reused one window, and the same window reopened from the tray after closing. Only fixture `status` and `ensure all` commands were used; production services were not stopped to simulate failure.

Screenshots show [normal](ready.png), [one inactive service](degraded.png) and [disconnected](offline.png) states. They contain owned fixture data. [Image digests](assets.json) identify the captures.

A separate owned Xvfb desktop had no notification area. [Its check](no-tray-validation.json) confirms the panel opened automatically and remained accessible, with the same state/timeout/recovery and single-instance checks. The native probe is also required in trusted CI.

The Rust node test changes a controlled service from inactive to active through private IPC and verifies aggregate health changes from `degraded` to `ready`. Host-node tests (11), packaging, formatting and Clippy passed locally. The application API and adapter protocol remain 1 / 5. This reports local process/service state, not phone connectivity or end-to-end model/media health.

Current tray support covers Xfce/X11 using GTK 3's platform adapter. Wayland/GNOME notification-area integration and macOS status menus remain pending. Public package hashes, trusted CI and deployment results are added after publication.
