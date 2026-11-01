# Agent notifications

Kite records task completion, attention, pending questions and task failure on the Linux host. Android keeps a durable inbox under **⋮ → Notifications** (通知); tapping an entry opens its saved host and conversation. Opening a notification does not answer a question, approve a tool, resume an archived conversation or submit a new model turn. Existing SSH trust still applies.

Allow Android's notification permission after connecting. The Notifications panel has an on/off switch and a link to Android's channel settings. Completion and attention use separate channels. Lockscreen content is generic; message bodies, prompts, terminal output and attachments are not included. A conversation currently being read is marked read without another system alert. Old bursts coalesce to the newest event for each conversation; the complete bounded inbox remains available.

## Delivery paths and current acceptance

The matching host and APK implement two paths with one event identity:

| Path | Requirement | Behavior |
| --- | --- | --- |
| Pinned SSH | A maintained connection | Core fetches durable events, saves its cursor and deduplicates reconnects. The Android connection service uses the connected-device foreground type. |
| FCM HTTP v1 | Working Google Play services, an Android Firebase app configuration, and host sender credentials | A high-priority data message starts the native messaging service after ordinary OS process reclamation. It persists the hint and posts the same system notification. A tap verifies the event and session over pinned SSH. |

FCM delivery requires real project/device configuration. A Release without that configuration explicitly shows **System push not configured** and must not be treated as supporting notifications after process reclamation. The implementation uses the supported HTTP v1 AndroidConfig path; the newer AndroidConfigV2 API is currently a preview and is not required. No Analytics dependency is added.

A system notification cannot bypass Android notification/channel restrictions or Settings → Force stop. Reopen after force stop. Google Play services and network availability are prerequisites for this FCM path; an OEM/UnifiedPush integration for devices without Play services remains a separate implementation.

## Configure Android

1. In the chosen Firebase project, register Android package `io.github.wenshiqi0.kite` and enable Cloud Messaging.
2. Set the repository **Actions variable** `KITE_FIREBASE_CONFIG_JSON` to that app's complete `google-services.json` content. This is the public Android client configuration, not a service-account key. Gradle validates the package and generates the SDK resources; no local configuration file enters Git or the Release attachments.
3. Build/publish the next Alpha using the standard [release procedure](releases.md). For a local compilation, supply the same environment variable. An unconfigured build remains buildable and makes no push registration request.
4. Allow notifications, connect to the host, and inspect **⋮ → Notifications**. It distinguishes missing Play services, missing permission, registration in progress, missing host credentials and retry states.

## Configure the Linux host

Create a Google service account allowed to send FCM messages to the same project. Its credential must be a regular file owned by the host user with mode `0600`, outside the repository and release assets, for example `~/.config/kite/fcm-service-account.json`. Configure the installed Node's JSON with an absolute path:

```json
{
  "push_service_account": "/home/YOUR_USER/.config/kite/fcm-service-account.json"
}
```

Add this field to the existing Node configuration, retaining its node ID, paths, services and environment; do not replace the configuration with this fragment. Restart only `kite-node.service` and reconnect the phone. `~/.local/bin/kite-node status` reports the notification API/last successful scan and push configuration/device/retry counts without returning tokens. A configured path alone is not evidence that credentials or actual FCM delivery work.

The device registers over the existing authenticated SSH connection. Each host stores its private token registry and delivery cursor in the Node state directory. Node installs its matching `push-v1.py` adapter beside `backend-v5.py`. OAuth tokens are cached privately on that host; credentials never go into the APK, GitHub variables, logs or downloads. Sender calls are bounded, unsuccessful sends back off, and unregistered device tokens are removed. A rotated device token keeps the event cursor. Turning notifications off suppresses local delivery, revokes the active host registration and deletes the SDK token; other hosts remove the old token when FCM reports it unregistered.

Node scans the catalogue independently of the phone's selected conversation. Archived, launch-pending and unowned sessions are excluded. Codex completion comes from a terminal lifecycle record, so a short turn is not lost between status polls. Initial idle state is not completion. Journal checkpoints survive restart and temporary agent-list omissions. Events are committed before publication; retained host history is bounded to 512 events, pages to 64, and the mobile inbox to 200. Source chat history is not pruned by these limits.

Notification journal or push registry errors preserve the affected file and leave normal chat/desktop available. Repair that private notification document independently; do not reset the host catalogue, SSH identity or conversation history.

## Acceptance checklist

Use an owned test conversation and retain its host/session/event identity. Check completion and a pending question in the background and on the lockscreen, duplicate SSH/push delivery, reconnect, denied permission, channel disable and the master switch. Tap real Android notifications in both warm and cold app starts, including a different saved host and an expired/deleted target. Verify no automatic response or approval occurs.

For process-reclamation acceptance, register a real FCM token, move Kite to the background, use `adb shell am kill io.github.wenshiqi0.kite` (not force-stop), generate a new host event with the phone process absent, and confirm a real notification arrives. Tap it and verify the exact conversation, history and identity. An injected service callback, emulator-local notice, green unit test or successful FCM HTTP response alone does not complete that acceptance.

References: [Android notification permission](https://developer.android.com/develop/ui/views/notifications/notification-permission), [foreground service types](https://developer.android.com/develop/background-work/services/fgs/service-types), [FCM Android receive behavior](https://firebase.google.com/docs/cloud-messaging/android/receive-messages), [FCM HTTP v1](https://firebase.google.com/docs/cloud-messaging/send/v1-api), [AndroidConfigV2 preview](https://firebase.google.com/docs/cloud-messaging/send/v2-api).
