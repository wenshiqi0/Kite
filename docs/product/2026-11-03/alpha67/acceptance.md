# Alpha 67 publication — 2026-11-03

[GitHub prerelease](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.67), versionCode **69**, source `192e65394c56bde508948a3433d87cd8798b3404`, includes merged PRs [#13](https://github.com/wenshiqi0/Kite/pull/13) and [#14](https://github.com/wenshiqi0/Kite/pull/14).

The tagged [build and publication workflow](https://github.com/wenshiqi0/Kite/actions/runs/41534302359) passed. All five public assets were downloaded again; GitHub asset digests, SHA256SUMS, manifest/source/version and file sizes matched. SDK verification confirmed the original certificate `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`, API-23 v1, API-36 primary v3 / compatibility v2 and 16 KiB alignment. Primary and compatibility APK application payloads are identical. Exact hashes and URLs are in [published-release.json](published-release.json).

The matching public Linux host archive was verified against its manifest and installed on wen.linux using the bundled Bun installer's compatible handover. The active Node is Alpha 67, healthy, and its running executable matches the public package digest. Node identity, configuration, SSH trust, Herdr/Sunshine PIDs, all six observed Codex owners and all 53 saved session references were preserved; systemd reports zero restarts. The new deletion endpoint rejects an invalid ID before mutation, proving that the new handler is served without deleting a user session.

Feature checks remain recorded in the [session deletion](../session-deletion/acceptance.md) and [automatic recovery](../android-auto-reconnect/acceptance.md) records. The source combination passed 147 Rust tests and 59 server tests, with 77 Android JVM tests and native-source/lint checks. Public signing verification and host deployment do not substitute for physical-phone installation, Doze/OEM battery behavior, network handover or device video-recovery acceptance.

[Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.67/kite-0.2.0-alpha.67-arm64-v8a-compat-v2.apk) · [Primary APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.67/kite-0.2.0-alpha.67-arm64-v8a.apk)
