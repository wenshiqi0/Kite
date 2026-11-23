# Antigravity quota monitoring & default Turbo

## Summary

- **Antigravity quota monitoring**: Kite advertises `usage: true` for verified `antigravity` hosts and wires `account/read` / `account/rateLimits/read` outside the owner's turn serialization queue so quota checks never block active turns, interrupts or approvals.
- **Official read-only data source**: `Antigravity.rateLimits()` runs the official headless read-only CLI command `agy -p /usage --output-format json` (`num_turns: 0`, no conversation created or tokens consumed), coalesces concurrent reads, and maps each model group (`Gemini Models`, `Claude and GPT models`, `Gemini 4 Argon`, etc.) in source order into Kite's `AccountUsage` contract with `remaining_percent`, `used_percent = 100 - remaining_percent`, window durations and reset timestamps. Unsupported auth (e.g. `GEMINI_API_KEY`) and signed-out states map to `Unavailable::UnsupportedAuth` / `Unavailable::NoAccount` without fabricating 0% or 100%.
- **Client UI**:
  - **Android**: Shows active-model quota summary in model settings, displays both remaining and used percentages (`剩余 X% · 已用 Y%`) per window, enables manual refresh for `antigravity`, uses provider-appropriate labels/messages, and hides Codex-only reset cards when absent.
  - **macOS**: Replaces the static single-window label with a clickable quota summary button in the composer bar opening a native popover with all quota groups, remaining/used percentages, progress bars, local reset times, status messages and a manual refresh button.
  - **Windows**: Formats window durations cleanly (`7天` / `5小时` / `N分钟`), displays remaining and used percentages with local reset times, and refreshes state after manual quota refresh.
- **Turbo as default**: New Antigravity sessions default `tool_permission` to `turbo` (`--dangerously-skip-permissions`), while preserving explicit per-session switching to `default` (host permissions) and cold-start policy restoration. Discovery probes (`inspect: true`) remain non-privileged.

## Verification

- `bun run check`: TypeScript, Biome, package boundaries and 140 tests passed (1 Darwin-only skip on Linux). Host regressions cover default Turbo on new sessions, explicit switch to `default` triggering approval, queued Turbo isolation, cold-start restoration, multi-group ordered Antigravity quota decoding (including 0% `omitempty` fraction -> `limit_reached`), `GEMINI_API_KEY` -> `unsupportedAuth`, signed-out -> `noAccount` and backend error propagation.
- `cargo test -p kite-herdr -p kite-agent --locked`: 114 tests passed (including `usage::tests::antigravity_ordered_buckets_and_unavailable_reasons_decode`).
- `cargo test -p kite-app --features host-node --locked`: 134 tests passed.
- `cargo check -p kite-ffi --locked` and `cargo check -p kite-ffi --features windows-client --locked`: passed.
- Live read-only check against installed Google Antigravity CLI **1.5.14** verified `tool_permission: "turbo"` default, `account: { type: "antigravity" }`, and real multi-group quota buckets (`Gemini Models`, `Claude and GPT models`, `Gemini 4 Argon`) with zero model turns submitted.
- Android, macOS and Windows native client compilation/packaging are validated by GitHub Actions PR CI.
