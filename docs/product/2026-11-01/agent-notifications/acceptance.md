# Agent notification delivery — Alpha 28

The requested boundary includes leaving Kite, lockscreen delivery, ordinary OS process reclamation, and tapping back into the correct conversation. This delivery adds an independent Linux event journal, durable mobile cursor/inbox, native privacy channels and permissions, verified host/session navigation, and FCM HTTP v1 sender/receiver integration.

Existing host profiles, SSH identity/trust, provider history and Sunshine pairing retain their formats. Notification state is separate. No old chat/FFI adapter is introduced. Node/API 1 and backend protocol 5 remain the shared baseline; notification API 1 is an additional explicit capability. Journal/push corruption preserves its file and does not prevent normal chat initialization. Host events are committed before publication; the transport carries only routing IDs, type and time, with no prompt, terminal or model-message body.

Source tests cover short Codex turn completion, startup idle, repeated polls/restarts, transient missing snapshots, pending questions, cursor bounds/order, token rotation/revocation, private credential files, OAuth reuse, provider rejection/retry, mobile persistence/deduplication, unknown host rejection and notification-store isolation. Native compilation, lint, instrumentation compilation, package checks and actual runtime outcomes are recorded below after publication.

**Full system-push acceptance is pending external configuration.** The repository currently has only Android signing settings, no Firebase app configuration. The Linux host has no configured FCM sender account, and working Play services on the user's phone has not been confirmed. The APK therefore reports system push as unconfigured. Connected SSH notifications or an injected native callback cannot establish post-reclamation push delivery. See [setup and precise acceptance](../../../notifications.md).

## Source validation before publication

- 106 Rust workspace tests passed with all host-node features, plus formatting and warnings-as-errors clippy.
- 21 backend tests and six publisher tests passed; the Linux package test passed.
- Android native Release compilation, unit-test compilation, instrumentation compilation and Release lint passed.
- All 52 compiled JVM tests passed through JUnitCore using Gradle's actual test runtime classpath. The local Gradle worker launcher is missing `worker.org.gradle.process.internal.worker.GradleWorkerMain`; it is not a product test failure. Trusted CI must still run the normal `testReleaseUnitTest` task before release acceptance.

No user APK was delivered locally. The following runtime/package sections are completed against the immutable public Release attachments after trusted CI publishes them.

## Immutable public package and installation

[Alpha 28](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.28) is built from `83f9c78fd39a690a337813f7c0caf3811164990a`, Android versionCode **30**. Main [41291970440](https://github.com/wenshiqi0/Kite/actions/runs/41291970440) and tag [41292066383](https://github.com/wenshiqi0/Kite/actions/runs/41292066383) both passed, including the normal Release JVM test runner and lint. The public APK has **23,387,232 bytes**, SHA-256 `21af597376fb7b271f2305be0a593bfe6bcc89a3936e546fe9cac7cb499c1c62`, original certificate SHA-256 `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`, explicit v1/v2/v3 signing and 16KiB alignment. Both public packages match their manifests/checksums. [Verification](published-release.json).

The exact APK was opened from Files and installed using Android 16's system PackageInstaller **Update** action. It displayed **App installed.** All 34 captured private files remained byte-identical before launch. There was no uninstall or data reset. [Installation](installer.json), [screenshot](system-installer.png). This is owned-emulator installation acceptance; the NX769J handset result is still separate.

## Host events and real Android notifications

The matching public Linux archive was installed with its node ID retained. The ten existing identity/configuration/owner files were unchanged; the embedded adapter advanced to its exact matching source. Original Herdr PID **36173** and Sunshine PID **1951255** were retained, as were seven pre-existing catalogue sessions. The tray/panel reopened and hid normally. Node reports notification API 1 with successful independent scans; push remains explicitly unconfigured.

An owned Codex conversation submitted one tool-free marker turn. The host journal observed its real `task_complete`, mobile Core persisted the event, and Android's NotificationManager posted a real system notification. Sending that notification's actual PendingIntent returned to the exact host/session and acknowledged it read. [Live test](live-notification.json), [instrumentation](live-notification.log). The owned provider source contains exactly one `task_started` and one `task_complete`; notification navigation submitted no extra model turn. [Source check](owned-source-check.json).

After leaving Kite and disconnecting its test SSH connection, `am kill` confirmed the app process was absent. The already-posted real notification remained visible in Android's notification drawer. Tapping it started Kite and rendered the owned conversation. A second cold-route check first cleared only the owned emulator's selection, then verified that the notification route restored the target selection over the host connection. [Cold tap](cold-start.json), [selection check](cold-route-selection.json), [returned conversation](cold-start-chat.png). These are cold-start navigation tests of an existing notification, **not delivery of a new FCM notification to a reclaimed process**.

Native NotificationManager checks passed for attention-channel selection, private/public lockscreen versions, deduplication, disable behavior and preservation of a separate connection notice. [Native check](native-notification.log). Its initial cancellation assertion sampled asynchronous Android cancellation too early; the test harness now waits up to two seconds and passes. This subsequent test-only adjustment does not change the public application's bytes or immutable source tag.

Cleanup removed the one owned provider thread/workspace/owner, temporary SSH authorization, local fixture catalogue/notice, test APK and current test download. Seven other catalogue sessions, the production services and ten existing private host files were retained. The owned emulator and relay were stopped; the main APK/data stayed installed. All five unrelated primary-checkout modifications were preserved. [Host cleanup](cleanup-host.json), [local cleanup](cleanup-local.json).

## Remaining acceptance

The selected end-to-end requirement remains **incomplete until real system push is configured and tested**. Neither Firebase app/sender configuration nor the handset's usable Google Play services has been supplied. No provider message was delivered in these tests. Use the [configuration guide](../../../notifications.md), then verify a newly generated event arrives while the phone process is absent and while locked. Different-host notification navigation, handset permission/channel restrictions and physical-phone delivery remain to be exercised on that configured environment. Audio/video/input performance was not re-benchmarked in this notification delivery.

## Final package — Alpha 29

The later test-harness revision required a new Alpha source/tag. [Alpha 29](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.29), versionCode **31**, is built from `8d0e8372ec8207133b4cc2e6178ae81b6e5bae7c`. Main [41294967167](https://github.com/wenshiqi0/Kite/actions/runs/41294967167) and tag [41294994860](https://github.com/wenshiqi0/Kite/actions/runs/41294994860) both passed. It is a prerelease, retaining the original certificate, explicit v1/v2/v3 signatures and 16KiB alignment. Public bytes: **23,387,232**, SHA-256 `290d230bec0c74adc4307ab54c71a5e0d9056b0cc6d03121727a7ab93d0d63f8`. [Independent verification](published-release-alpha29.json).

The two public APKs have the same 203 ZIP entries. All **four native libraries, three DEX files and 95 resource/asset entries are byte-identical** between Alpha 28 and 29. The only changed entries are `AndroidManifest.xml` and the three signing manifest/signature entries. The notification behavior tested above therefore uses the identical executable and resource payload in the final APK. Audio/video/input was not re-benchmarked. [Exact payload comparison](alpha28-alpha29-payload.json).

The exact public Alpha 29 APK was installed through Android 16 Files → PackageInstaller **Update**. The system reported **App installed.** Version advanced from Alpha 28/code 30 to Alpha 29/code 31; all **38** captured private files stayed byte-identical before launch. There was no uninstall or reset. [Installation](installer-alpha29.json), [system screenshot](system-installer-alpha29.png).

The matching public Linux host package is installed and healthy, retaining its node identity, original Herdr/Sunshine processes, configuration and existing owner records. The actual Alpha 28 completion event is still retrievable after the Alpha 29 Node restart, proving journal retention. [Host status](host-alpha29.json). Firebase sender configuration remains absent; full new-message delivery after process reclamation/lockscreen is still pending the project and device environment described above.
