# Rust crates and Bun server packages

Clients use native languages: Kotlin on Android, Swift on Apple platforms, Rust/GTK on Linux. Bun executes server adapters only. The Cargo workspace remains under `crates/*`; the Bun workspace lives under `packages/*`.

| Package | Owns |
| --- | --- |
| `@kite/protocol` | Checked JSON boundaries, session/model/native protocol types |
| `@kite/runtime` | Bounded subprocess/stream IO, native advisory locks, atomic state, process identity and JSON-RPC |
| `@kite/platform` | Linux X11 and keyboard mechanics, clipboard, cgroup mechanisms, private FCM transport |
| `@kite/providers` | Catalog, uploads/history, Codex WebSocket app-server, Claude terminal restore, DSH ACP, Ply MCP and durable session owners |
| `@kite/host` | Composition of the existing backend actions and platform entrypoints |

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

The Linux installer writes the absolute `bun` interpreter and `host.js` backend path, preserving host identity, SSH trust, provider files and user preferences. Existing configs are converted by the matching installer; runtime has no Python fallback. Host archives include both native executables. Installing requires the GTK 3 runtime libraries and Bun; Python is needed only to execute the installation/release tools.

The native panel loads the system GTK ABI with typed Rust bindings, uses one GApplication/D-Bus instance, and calls structured `kite-node` commands in worker threads. Its status polling and action queue are independent. Closing the panel/tray never stops host services. The GUI has no JS, Bun or Python runtime dependency.

For owned graphical acceptance, install Xvfb, xauth, xdotool and a session D-Bus:

```sh
xvfb-run -a dbus-run-session -- bun run scripts/native-desktop-probe.ts
KITE_QA_BUN="$(command -v bun)" xvfb-run -a dbus-run-session -- python3 desktop/linux/qa/agent_desktop_probe.py --binary target/debug/kite-node
```

Fixture tests use temporary homes, Unix sockets, local model catalogs and deterministic ACP/MCP peers. They cover state conflicts, immutable files, complete history paging, question controls, native image blocks, unique owners, approval reconnect, cancellation and bounded IO. The typed lifecycle contract preserves Alpha 51: only archived owners are released, with saved-context and alias checks under the input/restore/catalog leases; unarchived owners have no idle expiry. They do not spend provider credits or modify a running user node. Real provider/phone acceptance remains a separate release check.

## Local verification (2026-11-02)

- Frozen Bun installation; strict typecheck; Biome; AST-based package dependency validation: PASS.
- Bun protocol/platform/CLI tests: 28 passed, including full history paging, real Unix WebSocket framing, RS256 token signing/cache, concurrent catalog writes, uploads, ACP/MCP approvals, native cancellation, queued input and owner restart.
- Workspace Rust tests: 157 passed; Rustfmt and Clippy with warnings denied: PASS.
- Installer/package and Alpha publisher tests: 13 passed.
- Native Rust/GTK panel on owned X11/private D-Bus: screenshot visually checked, single-instance and GApplication actions passed.
- Real Rust Node → Bun → X11/standard MCP: pixels/crop metadata, UTF-8 and max-length typing, distinct Unicode batches with restored keymap, keyboard/chords, clicks, scroll and sanitized launch environment passed.
- Disposable cgroup kernel probe: a 128 MiB limit was enforced; the owned excessive allocation produced one OOM kill; the temporary scope/slice was removed.
- Real user-systemd Node/installer handover probe: continuous frames, exactly-once in-flight mutation, preserved agent PID, new-generation routing, failed-candidate recovery and single-writer notification/push handover passed.

These are local source checks and isolated integration results. This refactor has not been installed over the running user node, published as an Alpha, or verified with a real model/phone. `0.2.0-alpha.52` is reserved in source for the next matching release.
