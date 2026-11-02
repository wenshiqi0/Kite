# Linux host distribution assets

This directory contains the native panel's icons and localized strings. The panel itself is Rust/GTK in `crates/kite-desktop`.

Linux tooling uses Bun in [`@kite/linux-host`](../../packages/linux-host):

- [`src/install.ts`](../../packages/linux-host/src/install.ts) becomes the standalone `install.js` in each host archive. Install with `bun install.js --enable`.
- [`src/package.ts`](../../packages/linux-host/src/package.ts) creates the reproducible host archive from an explicit distribution allowlist.
- [`test/`](../../packages/linux-host/test) contains installer/package regressions, graphical fixtures and the owned X11/systemd acceptance programs.

The Linux package does not require Python. Android signing and release orchestration remain under `android/scripts/`.
