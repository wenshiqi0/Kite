# Linux taskbar status acceptance

The Linux host now has a resident notification-area icon. A green check, amber warning, red cross and clock badge communicate ready, degraded, unreachable and processing states. The menu and tooltip show Node and individual service states. Rust computes aggregate configured-service health; the native GTK UI presents it and clears old healthy content when Node disconnects or a status command times out.

The panel and tray share a three-second poll. Left-click opens the existing window; right-click opens status and actions. Closing the panel returns to a usable tray, and exiting the status UI leaves the node, external services and agents running. Desktop-login autostart uses the stable installed launcher and preserves existing user opt-out settings.

## Native checks

An isolated CLI fixture drove real GTK widgets on wen.linux's Xfce/X11 desktop. [The recorded check](xfce-validation.json) confirms the tray was embedded, its popup opened, service degradation changed the UI, a disconnect cleared stale healthy rows, a slow command timed out, service state recovered, repeated background and normal launches reused one window, and the same window reopened from the tray after closing. Only fixture `status` and `ensure all` commands were used; production services were not stopped to simulate failure.

Screenshots show [normal](ready.png), [one inactive service](degraded.png) and [disconnected](offline.png) states. They contain owned fixture data. [Image digests](assets.json) identify the captures.

A separate owned Xvfb desktop had no notification area. [Its check](no-tray-validation.json) confirms the panel opened automatically and remained accessible, with the same state/timeout/recovery and single-instance checks. Both checks passed with external GdkPixbuf modules disabled. Runtime icons are bundled PNG files; editable SVG sources stay in the repository. The same native probe is required in trusted CI with external image modules disabled.

The Rust node test changes a controlled service from inactive to active through private IPC and verifies aggregate health changes from `degraded` to `ready`. Host-node tests (11), packaging, formatting and Clippy passed locally. The application API and adapter protocol remain 1 / 5. This reports local process/service state, not phone connectivity or end-to-end model/media health.

Current tray support covers Xfce/X11 using GTK 3's platform adapter. Wayland/GNOME notification-area integration and macOS status menus remain pending. Published package hashes, trusted CI and deployment results follow below.

The first immutable Alpha 18 tag failed [native CI](https://github.com/wenshiqi0/Kite/actions/runs/41194252453) before APK packaging because the minimal Ubuntu image had no SVG decoder. It was not published. The corrected Alpha 19 source uses PNG resources and retains the failed tag unchanged.

## Published package and deployed status UI

[Alpha 19](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.19) is a public prerelease from **a17510096ad4d7e31845d49878273ab48e7dda5c**, Android versionCode **21**. Both [main CI](https://github.com/wenshiqi0/Kite/actions/runs/41194619428) and [tagged CI](https://github.com/wenshiqi0/Kite/actions/runs/41194716541) passed, including 86 Rust tests, Release JVM tests, lint, formatting/Clippy, 15 backend tests, 4 publisher tests, packaging and the native GTK probe without external image modules. The [publication record](published-release.json) includes exact hashes and independent v1/v2/v3, stable-certificate and 16KiB alignment verification of the public APK. Android functionality did not change, and this delivery did not repeat a device installation or mouse acceptance.

The **826,475-byte** Linux attachment has SHA-256 **8b4f0c138bee9a381013cfde1f51cafa54ab69c23f60737c570f19967967817b**. Its installed running executable matched the archive's binary digest **01f08cc2b626dcbfe7a0dea4631847cd116fa38a0f916a628af52043d644c2c9**. The [deployment check](official-deployment.json) confirms the previous node identity, original Herdr/Sunshine PIDs and 12 byte-identical private configuration/identity/catalogue/registry files. The desktop menu and login-startup entries passed desktop-file validation.

The official UI now runs as `kite-desktop-status.service` on wen.linux's actual Xfce session bus. The **22×22** [visible tray icon](official-tray.png) was identified by the installed process's window-owner PID. A normal launcher invocation opened the existing [management panel](official-panel.png); its window was observed to become unmapped when closed to the tray, while the same GUI and node processes continued. The original Alpha 17 panel instance was closed. Production Kite Node, the new status UI, Herdr and Sunshine remain running; owned Xvfb and private-bus fixtures exited. No production conversation was reset or used for a test prompt.
