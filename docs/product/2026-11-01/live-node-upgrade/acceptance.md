# Linux Node live-upgrade source acceptance

Source changes implement [RFC 0018](../../../rfcs/0018-live-node-upgrade.md) on the Alpha 31 source baseline. They are uncommitted local changes and not a published package. The final implementation has also been installed and tested on the local host; see the local deployment section below. The previous host identity, services and real sessions were not used as failure fixtures.

The real user-systemd [probe](../../../../desktop/linux/qa/live_upgrade_probe.py) ran the actual installer in a temporary user tree. Manager operations were confined to unique owned `kite-qa-upgrade-*` socket/service units. It tested the generated units rather than independently handwritten replacements. A trailing owned ELF marker changed the candidate's digest without rebuilding a second model/client; this proves process/digest/descriptor handover, not arbitrary future code/schema compatibility. API 1 / backend protocol 5 remain required.

[Recorded results](validation.json) show 985 successful framed request/response round trips, zero frame errors, a maximum measured round trip of 14.68 ms and an installer upgrade duration of 2.402 seconds while the old notification scan was allowed to complete. These are owned local transport measurements, not network or user-perceived latency guarantees.

The probe also verified:

- A blocked mutation completed exactly once across handover; its original connection remained open.
- A controlled detached agent retained its PID and continued advancing its heartbeat.
- New connections used the new service MainPID while the old generation served its existing connections, then exited after those connections ended.
- Incompatible executables, candidates that failed startup and unreadable candidate persistent state retained the old generation.
- A notifier failure after delivering MAINPID restored the old supervisor PID before stopping the candidate.
- Old/new notification writers did not overlap, a working-to-idle transition produced one completion event, and a concurrent push token update survived the handover.
- The installer used no explicit stop/restart during the compatible upgrade; owned units/processes were removed afterward.
- The replacement was adopted by the user manager before commit. Terminating that owned replacement while the old stream stayed open caused an automatic service restart, with no errors in the old stream.

GTK probes on isolated Xvfb/private D-Bus sessions verified status recovery, action scheduling/error retention, deferred reload while visible or busy, and loading the new UI on the next normal launch without a tray. The background idle reload decision was checked against the real application with only `execv` intercepted. No supported production desktop window was forcibly closed.

Validation passed: 114 Rust tests with `kite-app/host-node`, workspace Clippy (`-D warnings`), formatting, 27 backend Python tests, 5 installer/package Python tests and the native GTK probes. A cancellation/exec race in standalone listener ownership was fixed by explicitly unlocking the ownership file on scope exit; restart validation now waits for the new listener's actual response.

## Local deployment and real model acceptance

[Deployment evidence](deployment.json) records the explicit maintenance migration from the running Alpha 29 host to the local Alpha 31 test build. All nine pre-existing provider-owner PID/start-time identities remained intact. Herdr and Sunshine retained their original PIDs. Node configuration, authorized keys and SSH trust were unchanged. The session catalogue changed concurrently during the test; its current valid revision was preserved and never restored from a backup. The test created no catalogue entry.

The first local run exposed a supervision problem: when the replacement was still a child of a draining old generation, the user manager could fail to notice its death. An owned isolated crash test reproduced it. The final implementation adds the short-lived launcher, early process-identity handshake and adoption before MAINPID transfer; the isolated crash/restart test now passes. This fault injection never terminated the live host Node.

The patched build was hot-updated to the live host. A second owned ephemeral real Codex session then sent one bounded, tool-free test prompt through the installed Node proxy. Both subsequent debug/release handovers began while its turn was in progress. The same client connection, provider owner and loaded thread survived; the complete reply arrived with 4,319 characters and 1,599 delta events. Item types were only user/agent messages. Upgrade calls took 1.001 and 0.691 seconds; these are local test durations, not WAN or finger-to-photon latency. Both replacements had the actual user manager as their direct parent. The final running executable matches the optimized Release artifact digest.

The Xfce/X11 panel rendered correctly, its actual 22×22 tray icon was found, and hiding the panel retained the same GUI process across the hot upgrades. [Panel capture](panel-deployed.png) contains only the host management window. Owned Codex test processes and registry entries were removed; the final test generations drained. Recovery backups remain in the user's private cache, outside the repository.

Actual physical mobile clients, Sunshine video/audio/input, WAN/Tailscale and long-duration multi-upgrade workloads have not been accepted in this run. First migration from a deployed Node without handover still requires an explicit maintenance switch. No APK was built, released or installed.
