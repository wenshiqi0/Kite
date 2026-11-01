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
