# Codex terminal failures — Alpha 70

[Fix PR #15](https://github.com/wenshiqi0/Kite/pull/15) is merged into main as `174e8a3bea91223a2046d06dc4301561e892db8e`. The published [Alpha 70](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.70), versionCode **72**, is built from `85c88239502ebfd245f2b1250439e5befa2bed0b` on the latest published Alpha 69 base, retaining its Mac preview and Alpha 67 deletion/recovery behavior.

## Behavior and regression evidence

Codex `task_complete` records can carry an error. The host formerly discarded that error before notification classification and excluded the terminal record from conversation pages. Outcome controls now retain a failure flag without the error body and invalidate the old cache. Rust records a stable per-turn error with source position for history merging and persistence. Android displays a native failure row and capacity recovery hint; the Mac preview also recognizes the error message kind.

The fix branch passed all 60 server tests, Rust workspace tests, 82 host-node tests, Clippy and formatting. The release integration passed TypeScript strict/lint and 63 server tests, with one Darwin-only test explicitly skipped on Linux. Android passed 78 JVM tests, native test-source compilation and lint. Offline replay of the two real November 3 capacity failures recovered both terminal errors through three history pages and the Rust domain parser; no model request or provider-history mutation was made.

The owned Android 16 emulator passed `historicalTurnErrorsShowTheCauseAndRecoveryHint` (1 test), covering the capacity reason, recovery hint, a subsequent reply and missing-details fallback. The [native capture](codex-historical-capacity-error.png) contains synthetic conversation text. Its x86 test harness required a temporary scalar-CPU fallback in the pinned FEC dependency because of an existing NDK x86 linker failure; that local fixture change was restored and is absent from the tagged source and ARM64 package. This is native presentation acceptance, not installation of the final ARM64 APK on a physical handset.

## Published package and host

The tagged [Android build and publication](https://github.com/wenshiqi0/Kite/actions/runs/41581019593) passed. All five public assets were downloaded again and matched GitHub asset digests, `SHA256SUMS`, source/version metadata and sizes. SDK checks confirm the unchanged signing certificate, API-23 v1, API-36 primary v3 / compatibility v2 and 16 KiB alignment. The two APKs have identical application payloads. [Exact public verification](public-verification.json).

The matching official Linux package is installed on wen.linux through compatible handover. Its running executable matches the public host manifest; node identity, configuration, SSH authorization, seven observed Codex owners, all 54 captured session references and the external Herdr/Sunshine PIDs were preserved. systemd reports zero Node restarts. [Host checks](host-verification.json).

The installed host's actual history command was also replayed against the private snapshot: it retained the failed terminal outcome and excluded error bodies from notification controls. [Installed adapter checks](installed-adapter-verification.json).

A separate [macOS build/check](https://github.com/wenshiqi0/Kite/actions/runs/41581068774) checks the matching source and regenerated Swift boundary; its result is recorded separately from the Android/Linux release.

[Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.70/kite-0.2.0-alpha.70-arm64-v8a-compat-v2.apk) · [Primary APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.70/kite-0.2.0-alpha.70-arm64-v8a.apk)
