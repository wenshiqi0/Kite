# DSH integration acceptance

Source branch: `feat/dsh-harness`; published source `4daad6d66bf18603a26164a15c809c6aa23c458d`. Verified delivery: Android/Linux `0.2.0-alpha.43`, versionCode 45. This record separates source checks, real model execution, native presentation and public artifacts.

## Verified source and protocol checks

- 141 Rust workspace tests, Clippy with warnings denied and formatting checks passed.
- Android Kotlin, 71 JVM tests, lint and native-test source compilation passed. DSH initialization explicitly waits for its own managed chat connection.
- 40 host-adapter tests passed, including DSH opt-in and credential gates, authenticated model discovery, SDK identity checks, subprocess chat and tools, unique-owner reconnect, cross-session rejection and notification facts without message bodies. Five installer/package tests and eight publisher tests passed.
- Internal harness instructions remain behind one AGENTS trigger and development links. Detailed server/client/DSH guides are not loaded into common Skill, MCP initialization or desktop tool instructions.

## Verified real DSH execution

An isolated SDK profile used an already available model credential through an environment variable. No credential value, user conversation or hidden reasoning is included here.

1. A native SDK handshake and a real model turn returned an owned random marker; a second turn in the same SDK session remembered it without tools.
2. The Kite-managed owner passed authenticated availability discovery, then used a native file-reading tool on an owned fixture and stored linked public tool/result messages.
3. Reopening the same logical conversation retained the same owner PID and reference. The next real turn remembered the fixture marker; canonical history contained exactly two user messages and one tool call.
4. Only the owned worker, native subprocesses, fixture and temporary SDK profile were cleaned up. Existing provider owners and user configuration were not changed by these checks.

DSH SDK exposes creation, prompt and notifications rather than an SDK resume RPC. Kite therefore preserves its own logical conversation and public history; normal owner reconnection reuses the SDK session, while a new SDK process receives saved public context once before a new explicit input. This is not native SDK-memory restoration.

## Native presentation and delivery

Two native API 36 tests passed on an isolated x86_64 emulator: verified DSH is selectable with ID `dsh`, absent providers do not appear, and initialization prevents input until ready. These are actual component/initialization checks with synthetic display metadata, not a claim of a full physical-phone model turn. The x86 UI harness uses a temporary portable Reed–Solomon CPU-dispatch header for an existing nanors/Clang link limitation; production ARM64 sources and optimization settings are unchanged. [Native record](native-validation.json).

[Alpha 43](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.43) was published by [tag CI](https://github.com/wenshiqi0/Kite/actions/runs/41342566322). Independent public downloads passed checksum, version/code, stable certificate versus Alpha 42, primary/v2-compatible payload identity and 16 KiB ZIP-alignment checks. [Public artifact record](published-release.json).

The verified matching Linux package was installed through compatible live handover. Node reports `0.2.0-alpha.43`, `harnessApi=1`, ready health and available `codex`, `claude`, `dsh`. Host identity and external service PIDs were retained. DSH uses private Node environment and a Kite-only SDK profile, leaving global DSH model settings unchanged. [Host record](host-validation.json).

Physical-handset installation, WAN conditions and newly delivered system push remain separate from the isolated model and emulator checks. DSH supports the declared text/tool/history flow; unsupported interaction and input features remain unavailable.
