# RFC 0024: Bun server packages alongside Rust crates

Status: implemented, 2026-11-02.

Keep the Cargo workspace, Rust client/domain state, SSH/IPC framing and native presentation boundaries. Replace the Python server adapters with strictly checked TypeScript executed by Bun. Installation/release and QA tooling may continue using Python. Client UI is native: Kotlin, Swift and Rust/GTK. No server action dispatches to a Python adapter.

The root Bun workspace contains `@kite/protocol` (wire contracts and validation), `@kite/runtime` (bounded IO, atomic persistence, locks and transports), `@kite/platform` (desktop, clipboard, cgroups and FCM mechanisms), `@kite/providers` (Codex, Claude terminal restoration, DSH ACP and Ply native MCP), and `@kite/host` (CLI composition). Package dependency direction is explicit in package.json; provider execution loops remain native.

Preserve backend protocol 5, harness API 2, saved catalog revision 1, session references, original provider logs, upload identities, process start-time checks, the Alpha 51 archived-only lifecycle contract, revalidation and resource admission. Bun replaces the interpreter configuration field (`bun` instead of `python`); the matching installer writes the new configuration. There is no runtime Python fallback. Existing live owners remain untouched during source development; generation-pinned bundles allow old processes to drain when a matching host is installed.

Cargo builds bundle TypeScript with Bun into OUT_DIR, then embed the host, DSH plugin and clipboard bundles. Installed services extract immutable content-addressed generations. No build output is committed. TypeScript strict checks run separately from transpilation. CI runs frozen Bun installation, typecheck, lint, fixture/integration tests and the existing relevant Rust checks. Test fixtures never require account credentials, a model request or the user's desktop.

The Linux panel and tray use `crates/kite-desktop`, a native Rust GTK 3 executable. Kotlin and Swift clients remain native. GTK is accessed through typed system ABI bindings; the desktop executable has no Bun/TypeScript runtime dependency. Installation, packaging and QA tooling are the only remaining Python code.
