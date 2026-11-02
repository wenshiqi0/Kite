# Rust crates and Bun server packages

Clients use native languages: Kotlin on Android, Swift on Apple platforms, Rust/GTK on Linux. Bun executes server adapters and Linux installation, packaging and acceptance tools. Every TypeScript package keeps production code in `src/` and tests/fixtures in `test/`; dependency checks reject production imports of test code. The Cargo workspace remains under `crates/*`; the Bun workspace lives under `packages/*`.

| Package | Owns |
| --- | --- |
| `@kite/protocol` | Checked JSON boundaries, session/model/native protocol types |
| `@kite/runtime` | Bounded subprocess/stream IO, native advisory locks, atomic state, process identity and JSON-RPC |
| `@kite/platform` | Linux X11 and keyboard mechanics, clipboard, cgroup mechanisms, private FCM transport |
| `@kite/providers` | Catalog, uploads/history, Codex WebSocket app-server, Claude terminal restore, DSH ACP, Ply ACP and durable session owners |
| `@kite/host` | Composition of the existing backend actions and platform entrypoints |
| `@kite/linux-host` | Standalone Bun installer, reproducible host packaging and owned Linux acceptance tools |

Install Bun 1.3.14 or newer and run:

```sh
bun install --frozen-lockfile
bun run check
bun run build
cargo test -p kite-agent -p kite-app --features kite-app/host-node -p kite-desktop --locked
cargo clippy -p kite-node -p kite-desktop -p kite-agent -p kite-app --features kite-app/host-node --all-targets --locked -- -D warnings
cargo build -p kite-node -p kite-desktop --locked
```

Cargo bundles the server into `OUT_DIR`; it does not run dependency installation or emit committed JS. Run strict `tsc` independently: Bun transpilation is not the type-check gate. Frozen installation pins lint, types and compiler versions. Cross-compiling a Rust client also needs Bun on the build machine to produce the embedded bundle.

The Linux installer writes the absolute `bun` interpreter and `host.js` backend path, preserving host identity, SSH trust, provider files and user preferences. Existing configs are converted by the matching installer; runtime has no Python fallback. Host archives include both native executables. Installing requires the GTK 3 runtime libraries and Bun; Linux tools and the shipped `install.js` require Bun rather than Python. The Android signing/publishing orchestrator remains a separate Python tool under `android/scripts/`.

The native panel loads the system GTK ABI with typed Rust bindings, uses one GApplication/D-Bus instance, and calls structured `kite-node` commands in worker threads. Its status polling and action queue are independent. Closing the panel/tray never stops host services. The GUI has no JS, Bun or Python runtime dependency.

For owned graphical acceptance, install Xvfb, xauth, xdotool and a session D-Bus:

```sh
xvfb-run -a dbus-run-session -- bun run packages/linux-host/test/qa/native-panel.ts
xvfb-run -a dbus-run-session -- bun run packages/linux-host/test/qa/agent-desktop.ts --binary target/debug/kite-node
```

Fixture tests use temporary homes, Unix sockets, local model catalogs and deterministic ACP/MCP peers. They cover state conflicts, immutable files, complete history paging, question controls, native image blocks, unique owners, approval reconnect, cancellation and bounded IO. The typed lifecycle contract preserves Alpha 51: only archived owners are released, with saved-context and alias checks under the input/restore/catalog leases; unarchived owners have no idle expiry. They do not spend provider credits or modify a running user node. Real provider/phone acceptance remains a separate release check.

## Local verification (2026-11-02)

- Frozen Bun installation; strict typecheck; Biome; AST-based package dependency validation: PASS.
- Bun package and distribution tests: 45 passed, including full history paging, real Unix WebSocket framing, RS256 token signing/cache, concurrent catalog writes, uploads, ACP/MCP approvals, native cancellation, queued input and owner restart.
- Workspace Rust tests: 160 passed; Rustfmt and Clippy with warnings denied: PASS.
- Linux installer/package coverage is included in the Bun suite; Android Alpha publisher tests: 9 passed.
- Native Rust/GTK panel on owned X11/private D-Bus: screenshot visually checked, single-instance and GApplication actions passed.
- Real Rust Node → Bun → X11/standard MCP: pixels/crop metadata, UTF-8 and max-length typing, distinct Unicode batches with restored keymap, keyboard/chords, clicks, scroll and sanitized launch environment passed.
- Disposable cgroup kernel probe: a 128 MiB limit was enforced; the owned excessive allocation produced one OOM kill; the temporary scope/slice was removed.
- Real user-systemd Node/installer handover probe: continuous frames, exactly-once in-flight mutation, preserved agent PID, new-generation routing, failed-candidate recovery and single-writer notification/push handover passed.

The Bun server refactor was published and installed on wen.linux as Alpha 55. Its public host binary matched the running executable, Codex/DSH/Ply real requests passed, the existing Codex connection survived handover, and session 31d3 retained its original owner/thread. Alpha 60 adds the source/test layout and Python-free Linux distribution/QA tools. These checks do not claim physical-phone UI/media acceptance.
