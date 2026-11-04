# Android and Linux Alpha releases

The GitHub Actions workflow is [Android Release](../.github/workflows/android-release.yml).

## What runs

| Event | Build | Delivery |
| --- | --- | --- |
| Relevant changes on `main` | Signed APK and Linux host archive, Rust/native UI checks and Android lint | Verified workflow artifact |
| Pull request to `main` | Unsigned APK, Rust/native UI checks and Android lint | Unsigned PR artifact; no signing secrets |
| Annotated `vX.Y.Z-alpha.N` tag | Signed APK and Linux host archive from the tag's source commit | GitHub prerelease with APK, Linux host archive, checksums and manifest |
| Manual workflow dispatch | Signed Release APK | Verified workflow artifact |

Only tagged builds publish a user-facing GitHub Release. They are always prereleases and are never marked as the latest stable release. Workflow artifacts are retained for 14 days, or 7 days for unsigned PR builds.

## Verified delivery — 2026-11-04

[0.2.0-alpha.83](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.83) / versionCode **85** adds expandable Codex subagent trees and individual activity states. Source and tagged CI checks pass; public checksums, stable signatures, API-23/36 verification, identical APK payloads and alignment are independently verified. The matching public Linux host is installed with all 78 saved references and three active Codex owners preserved, and installed discovery returns the actual worker. Android handset and macOS native rendering remain unverified; the x86_64 Android component run is blocked by the existing Moonlight linker issue. [Evidence](product/2026-11-04/codex-subagents/acceptance.md).

## Verified delivery — 2026-11-03

[0.2.0-alpha.73](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.73) / versionCode **75** displays Codex asynchronous user messages as normal assistant text, including existing offline history, while preserving raw tool records for paging and reconnect. Malformed payloads and failed deliveries retain their diagnostic details. It builds on Alpha 72; only the client needs updating. Seventy local agent/FFI tests, formatting, strict Clippy and both tagged build/publication jobs pass. Public manifest/checksum metadata matches the source commit, stable certificate pin and APK asset digests. [Evidence](product/2026-11-03/async-user-messages/acceptance.md).

[0.2.0-alpha.70](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.70) / versionCode **72** fixes silent Codex terminal failures: errors remain in conversation history after reconnect/continuation, old outcome caches rebuild, and failed turns produce failure notifications. It is based on published Alpha 69 and retains its existing features. Fix PR #15 is merged; server/Rust/Android checks and the owned Android 16 error presentation pass. Tagged CI and independent public signing/checksum/payload/alignment verification pass. The matching official Linux host is installed with seven captured Codex owners and all 54 session references preserved. Update both host and client. [Evidence](product/2026-11-03/codex-turn-failures/acceptance.md).

[0.2.0-alpha.67](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.67) / versionCode **69** combines direct session deletion and Android automatic connection recovery from PRs #13 and #14. Deletion stops verified owners and tools before removing Kite records; failures retain the record, and stale clients cannot recreate deleted entries. Android retains its foreground service during recovery, wakes on network/foreground changes, and restores explicit connection intent without replaying model work. Manual disconnect cancels recovery. Both client and host must be updated. Source checks include 147 combined Rust tests, 59 server tests, 77 JVM tests, native test-source compilation and lint; tagged CI signs and verifies the published assets. Tagged CI and fresh public checksum/certificate/API-23/36/payload/alignment checks pass. The matching official Linux host is installed by compatible handover; six observed owners and all 53 session references remain intact. Physical-device Doze/OEM battery policy and network handover remain separate acceptance. [Publication evidence](product/2026-11-03/alpha67/acceptance.md).

## Verified delivery — 2026-11-02

[0.2.0-alpha.65](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.65) / versionCode **67**, source `9e6f72625146739982208f08d74c30339ae06cda`, rebuilds the installers following the handset's Alpha 64 `-103`. Main/tag CI and fresh public checksum, original-certificate, API-23/36 signature and 16 KiB alignment checks pass. Only the manifest and signing data differ from Alpha 64; DEX, resources and native libraries match. Prefer the [v2 compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.65/kite-0.2.0-alpha.65-arm64-v8a-compat-v2.apk). Handset installation remains pending; the running host need not change. [Evidence](product/2026-11-02/apk-resign-alpha65/acceptance.md).

[0.2.0-alpha.51](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.51) / versionCode **53**, source `8084dd4d107d903afb0f8c5338df4133883a425f`, adds a shared harness resource-release contract, Android’s archived-process release action and `kite sessions release-archived`. Archived idle owners are released with original context retained; unarchived sessions no longer expire for inactivity. Rust/Clippy, 71 backend tests, 73 JVM tests, lint and native test-source compilation pass. Tagged and PR CI pass; public signatures/checksums/payload/alignment are independently verified. Matching official Linux host is installed, and real DSH/Ply owners release and restore without a model turn. [Evidence](product/2026-11-02/archived-resource-release/acceptance.md).

[0.2.0-alpha.50](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.50) / versionCode **52**, source `fc9d0df3635a9c797be5469d96776f4d5e9c4c10`, aligns DSH/Ply model, reasoning and native vision controls, adds Linux Ply, switches to verified harness dropdowns, and fixes DSH supplemental input through native steering. Tag CI, public artifact verification, real native model/control checks, three Android component tests, and actual budgeted DSH/Ply owner startup checks pass. The matching host is installed with identity and existing services retained. [Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.50/kite-0.2.0-alpha.50-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-02/harness-parity/acceptance.md).

[0.2.0-alpha.46](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.46) / versionCode **48**, source `898a659d2550445ab1fdb90e2dfccda01f3b4408`, adds host CPU/memory profiles, cache-aware memory warnings, native background alerts and a configurable aggregate agent budget. The local 10 GiB budget is kernel-enforced with startup reservations and migration accounting; the control plane is separate. Workspace/Clippy, 58 backend tests, 73 JVM tests, lint, three native API 36 tests and real cgroup/SDK-scope probes pass. Main/tag CI and independent public checksums, stable certificate and alignment checks pass. Matching CI Linux host is installed. [Evidence](product/2026-11-02/host-resources/acceptance.md).

[0.2.0-alpha.45](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.45) / versionCode **47**, source `3c40b2dd44d9ffc98276b11adc790f34a8c3ab5d`, is the final combined DSH client/host delivery. It retains the session lifecycle API, passes tag CI and public signature/checksum/payload/alignment verification, and is installed on the matching host with DSH available and existing identity/external services preserved. Android product code is unchanged from the verified Alpha 43 DSH UI. [Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.45/kite-0.2.0-alpha.45-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-02/dsh/acceptance.md).

[0.2.0-alpha.43](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.43) / versionCode **45**, source `4daad6d66bf18603a26164a15c809c6aa23c458d`, adds opt-in DSH SDK conversations and host-verified provider choices. Real model/tool/reconnect checks, native provider-choice/initialization checks and tagged CI passed. Public artifact checks confirm stable signing identity, matching primary/compatibility contents and alignment. The matching host is installed, DSH is available, and existing identity/external services are retained. [Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.43/kite-0.2.0-alpha.43-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-02/dsh/acceptance.md).

[0.2.0-alpha.42](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.42) / versionCode **44**, source `09a8efd69753462f163406a88c1cef94f9c8d333`, uses friendly skill names and short summaries in the picker, search, selected chips and accessibility labels while preserving original invocation identifiers. 141 Rust tests, 70 JVM tests, Clippy/lint, a real metadata-loader fixture and three native Android 16 skill-picker tests pass. Main CI and the third tag attempt pass; two earlier existing Unicode-input probe failures and two passing local rechecks are retained in the evidence. Public checksum, stable-certificate, equal primary/compatibility payload, packaged helper and 16 KiB alignment checks pass. [APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.42/kite-0.2.0-alpha.42-arm64-v8a.apk) · [Evidence](product/2026-11-02/skill-display-metadata/acceptance.md).

[0.2.0-alpha.41](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.41) / versionCode **43**, source `1db76499c61273f29ec0f83f58458d7b0bf6a076`, combines the icon-only latest control and atomic echo merge with one initialization notice and locked submission until launch/chat readiness. 140 Rust tests, 68 JVM tests, Clippy/lint and three native Android 16 presentation tests pass. Tagged build/publication and independent public checksum, stable-certificate, equal primary/compatibility payload and native-alignment checks pass. Native runtime uses a matching x86 debug harness; physical-handset acceptance remains separate. [Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.41/kite-0.2.0-alpha.41-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-02/session-initialization/acceptance.md).

[0.2.0-alpha.40](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.40) / versionCode **42**, source `11499996d5054714c7b35cce7d0be516e1c96943`, provides the icon-only latest button and echo reconciliation. The added initialization request is included in Alpha 41. [Evidence](product/2026-11-02/latest-icon-and-message-echo/acceptance.md).

[0.2.0-alpha.39](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.39) / versionCode **41**, source `085fa0acaff717bcbc3581d679db31839c2197bd`, adds compact inline-code backgrounds and joined gray/blue question-and-answer cards, including history and immediate successful-answer display. Rust/JVM/lint checks, six native Android 16 presentation tests and main/tag CI pass. Native captures cover nine width/font combinations and actual paired-panel colors. Independent public APK checks verify stable signatures, equal primary/compatibility payloads, checksums and 16 KiB native ZIP alignment. Physical handset acceptance remains separate. [Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.39/kite-0.2.0-alpha.39-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-02/markdown-and-question-replies/acceptance.md).

## Verified delivery — 2026-11-01

[0.2.0-alpha.38](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.38) / versionCode **40**, source `7442493b55127053f11ec24aa0e6a6028d6a1862`, combines explicit question waiting, 240/96 dp verified-tail navigation, CJK strong-markup repair and draft/history image/audio/text previews. Integrated Android/JVM/native-source checks and main/tag CI pass. Public signature/checksum/payload/alignment checks pass. The matching host is installed with config/identity and external service PIDs retained; real text/WAV/received-JPEG transfers pass. Phone preview/gesture/playback execution remains pending. [Compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.38/kite-0.2.0-alpha.38-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-01/attachment-preview-and-markdown/acceptance.md).

[0.2.0-alpha.33](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.33) / versionCode **35**, source `2d582141b439aa37e6b67fdbee902b7dd4cc6af0`, adds an explicit `-compat-v2.apk` signing test for the handset's Alpha 32 `-103`. The primary APK retains v1/v2/v3; the compatibility APK uses v1/v2 with the same certificate, version and application contents. Main/tag CI, eight publisher tests and independent public-asset checks pass; API 36 selects v2 for the compatibility asset. Handset installation remains pending; a v3 device defect is not established. [Direct compatibility download](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.33/kite-0.2.0-alpha.33-arm64-v8a-compat-v2.apk) · [Evidence](product/2026-11-01/signature-compatibility/acceptance.md).

[0.2.0-alpha.32](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.32) / versionCode **34**, source `f44bd41f8a67303e9a5b04ff66d58f625a85db0f`, keeps public Codex commentary/interim conclusions visible as tools arrive and adds available reset-card count plus earliest-expiry countdown/date to allowance preview and details. Client JVM regressions, native-source compilation, 116 Rust tests, main/tag CI and public checksum/certificate/alignment checks pass. Live read-only quota fields were confirmed without redemption; private account facts stay outside the repository. Device UI execution and installation remain separate acceptance. [Evidence](product/2026-11-01/readable-progress-and-cards/acceptance.md).

[0.2.0-alpha.31](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.31) / versionCode **33**, source `2850a07a7581736c6279fc70d5622102359c21c8`, adds Android session activity labels and native Codex image preview, zoom, save and offline caching. Main/tag CI, Android JVM tests and native-test compilation pass. Public APK checksums, stable certificate and alignment pass; the downloaded host package returns the real generated PNG through an isolated Node instance. Both Android and the host require this update. Device preview/zoom/export interaction remains pending. Alpha 30 is an unpublished superseded candidate with its original tag retained. [Evidence](product/2026-11-01/session-images/acceptance.md).

[0.2.0-alpha.29](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.29) / versionCode **31**, source `8d0e8372ec8207133b4cc2e6178ae81b6e5bae7c`, is the final notification validation package. Main/tag CI and public signing/checksum/alignment checks pass. All native libraries, DEX and resources match the runtime-tested Alpha 28 exactly; only manifest version/signing entries differ. Android 16 system-installer update retained all 38 captured private files. The matching Linux node is installed with prior journal events retained. Real new FCM delivery after OS process reclamation remains pending project/device configuration. [Final acceptance](product/2026-11-01/agent-notifications/acceptance.md#final-package--alpha-29).

[0.2.0-alpha.28](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.28) / versionCode **30**, source `83f9c78fd39a690a337813f7c0caf3811164990a`, adds durable host/mobile agent notifications, privacy channels, permission/settings, verified conversation navigation and configurable FCM transport. Main/tag CI, public signatures/alignment/checksums and Android 16 system-installer update passed, retaining all 34 captured private files. A real host completion reached Android; actual notification taps passed warm and cold conversation return. The matching Linux node is installed with existing services/identities and seven other sessions retained. **New system-push delivery after process reclamation remains pending Firebase/device configuration.** [Acceptance](product/2026-11-01/agent-notifications/acceptance.md).

[0.2.0-alpha.27](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.27) / versionCode **29**, source `87e41cffbffe6133198d8544bb8d5383eee94b10`, is a **re-signed installer test** following the handset's repeated `-103`. The original certificate remains valid through March 15, 2054. CI now performs explicit SDK re-signing with v1/v2/v3 and a non-signing-payload equality guard. Independent public-package checks and an Android 16 Files → system PackageInstaller in-place update pass, retaining all 30 captured private files. Handset acceptance remains pending. Features match Alpha 26 and the installed backend stays compatible without restart. [Evidence](product/2026-11-01/apk-resign/acceptance.md).

[0.2.0-alpha.26](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.26) / versionCode **28**, source `ad1ecfda2797a290144df52de1bfd523ec282448`, is the completed new-session/audio/recovery delivery. Public-package signatures/alignment/checksums and in-place retention pass. The exact APK passed first-message/reconnect, sustained non-silent audio (99.1% sampled real-time playback, zero queue drops), mute/focus/teardown, actual 1080p video and interrupted-host recovery. The matching Linux node/panel is installed; existing identity, history and external owners/services are retained. Alpha 24's audio candidate failed sustained playback and is documented as superseded. Physical-phone/WAN acceptance remains separate. [Final evidence](product/2026-11-01/new-session-and-recovery/acceptance.md).

[0.2.0-alpha.23](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.23) / versionCode **25** is built from `4227128550b2aba748cc757157beafbe885910b5`. It replaces quiet-time mouse deferral with fixed 16ms pointer/wheel delivery, retains fixed 200ms text batching, preserves source pixels with a conservative Tailscale bitrate budget, and reduces decoder queue pressure. Main/tag CI, independent signing/checksum/alignment verification and in-place retention checks passed. The public APK sustained 1080p at 54.6 rendered FPS in the owned local scene, and continuous pointer input reached the host throughout the gesture. The matching Linux node/panel is installed with identity/configuration and original Herdr/Sunshine processes retained. Physical-phone Tailscale responsiveness remains unverified. Exact evidence is recorded in [performance acceptance](product/2026-11-01/streaming-performance/acceptance.md).

[0.2.0-alpha.22](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.22) / versionCode **24** is the final package from `1d5fe46eed861be576426862cdefcc9c4af81bc7`. Main/tag CI and independent public-package verification passed. Production feature sources and all Android native libraries match the Alpha 21 acceptance below; version/source metadata, test harness and documentation advance. The matching Linux package is installed with identity/configuration and external services retained. Alpha 22 did not repeat device installation or native/live interaction checks. [Final publication record](product/2026-11-01/chat-tail-and-allowance/final-release.json).

[0.2.0-alpha.21](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.21) / versionCode **23** was published from `9d2f948b3dc8a8a2c5e2659069eb2d5a4f71b73f`. Main/tag CI passed. The public APK passed independent stable v1/v2/v3 signing and 16KiB alignment checks, in-place data retention, two native presentation tests and a live allowance/setup-disconnect test. The matching Linux package was installed with the existing node identity and original Herdr/Sunshine processes retained. See [acceptance](product/2026-11-01/chat-tail-and-allowance/acceptance.md) and [exact publication evidence](product/2026-11-01/chat-tail-and-allowance/published-release.json). Active-video-frame and physical-phone ergonomics were not repeated in this delivery.

## Verified delivery — 2026-10-31

[0.2.0-alpha.19](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.19) / Android versionCode **21** was published from `a17510096ad4d7e31845d49878273ab48e7dda5c`. Main/tag CI passed. Both public packages matched their checksums; the APK passed independent stable v1/v2/v3 signing and 16KiB alignment checks. The matching Linux node and native taskbar UI were installed on wen.linux, with existing service processes and private identity/configuration records retained. This delivery did not repeat an Android device installation. See [taskbar acceptance](product/2026-10-31/linux-taskbar/acceptance.md) and its [publication record](product/2026-10-31/linux-taskbar/published-release.json).

## Verified delivery — 2026-10-30

[0.2.0-alpha.11](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.11) / versionCode **13** was published from source `1bd806f15b0c49842f857b5128151a028fca052d`. Both main and tagged CI passed. The public attachment was downloaded, matched against its manifest, independently verified for v1/v2/v3 signatures and 16 KiB alignment, and installed as an update on the Android 16 emulator. Reconnection reused the existing SSH identity and desktop pairing, and automatic text clipboard sharing passed in both directions. See the [acceptance record](product/2026-10-30/hybrid-input/acceptance.md) and [machine-readable evidence](product/2026-10-30/hybrid-input/published-release.json) for exact bytes, checksum, certificate and CI links. Physical-phone acceptance of Alpha 11 is still pending.

## Compatibility APK — Alpha 33+

Each signed release also includes `kite-X.Y.Z-alpha.N-arm64-v8a-compat-v2.apk`. This diagnostic asset uses the same certificate, version and non-signing payload as the primary APK, with v1/v2 enabled and v3/v4 disabled. The primary APK retains v1/v2/v3. The compatibility gate separately forces API-36 v2 and API-23 v1 verification, then compares certificate and payload identity. Both APKs, the host archive and manifest are listed in `SHA256SUMS`; publication verifies all five assets. Use the explicit compatibility download when diagnosing a handset `-103`, and record handset acceptance separately from SDK checks.

## Version and signing identity

`android/version.properties` is the Android version source. Increment both the Alpha suffix and `versionCode` for each new delivery. The initial CI release is `0.2.0-alpha.1` / `3`. APK metadata must match the tag exactly.

Alpha uses the same signing identity as the earlier test APKs so existing installations can upgrade without replacing their device SSH key, hosts or trust records. The keystore remains outside Git. CI receives it through an encrypted Actions secret; it is never an artifact.

Alpha 10 explicitly enables APK signing schemes v1, v2 and v3 with this same key. The publisher requires all three schemes to verify and checks 16 KiB ZIP alignment before creating the release manifest. These checks cover the reported `-103` certificate-collection failure path; a handset's downloaded bytes and installer behavior still require device verification. Signature schemes and ZIP alignment are recorded in the manifest.

The user subsequently reported that Alpha 10 installed successfully on their Android 16 phone. This is recorded as user-reported installation success in the [Alpha 10 investigation](product/2026-10-30/install-signatures/acceptance.md); the exact earlier handset failure was not reproduced from device logs.

The JAR/v1 check runs separately with the verifier constrained to API 23, because the normal minimum-API-26 verification can skip the v1 path even when it is present. Both verification passes must report the same certificate. This does not lower the app's minimum supported Android version.

Configure these repository settings before the first signed build:

| Setting | Kind | Purpose |
| --- | --- | --- |
| `ANDROID_ALPHA_KEYSTORE_BASE64` | Required Actions secret | Base64-encoded existing keystore |
| `ANDROID_ALPHA_CERT_SHA256` | Required Actions variable | Expected signing certificate SHA-256 |
| `ANDROID_ALPHA_STORE_PASSWORD` | Optional Actions secret | Store password; Alpha's existing test-key default is `android` |
| `ANDROID_ALPHA_KEY_PASSWORD` | Optional Actions secret | Key password; existing default is `android` |
| `ANDROID_ALPHA_KEY_ALIAS` | Optional Actions secret | Existing default is `androiddebugkey` |

These settings have been configured for `wenshiqi0/Kite`. Keep the signing identity stable. Alpha's existing test certificate is for GitHub distribution, not an app-store production signing identity.

## Publish a version

1. Update `android/version.properties`, finish validation, and commit the complete source.
2. Write release notes describing behavior, validation and current limitations. The notes become the annotated tag body.
3. From a clean source branch, run:

```bash
python3 android/scripts/release-alpha.py --notes-file /absolute/path/release-notes.md
```

The command pushes the source branch and annotated tag, waits for the tag workflow, and reports the published Release URL. The tagged CI build is the publication authority; local builds do not race it to upload a different APK.

Alpha 27+ also runs a final explicit SDK `apksigner` pass using the same certificate after alignment, with environment-only passwords and a non-signing-payload equality guard. No APK mutation is permitted after this final signing step. This is an installer diagnostic change, not a new signing identity or a confirmed fix for a specific handset failure.

The build verifies the APK's version, non-debuggable Release variant, ABI, signature and pinned certificate. The publish job checks the source commit and checksums, uploads a draft prerelease, downloads every asset to verify its bytes, and then publishes it. An existing tag cannot move to another commit, and existing release assets cannot be replaced with different bytes.

If a publish step is interrupted, rerun that failed job from GitHub Actions so it reuses the same verified build artifact. If source or artifact content changes, use a new Alpha version.

## Synchronized Linux host package

Alpha 17+ packages the Linux x86_64 host from the same commit as Android. Trusted CI builds `kite-node`, packages only the binary/native panel/bundled Bun installer (`install.js`)/license/host manifest, and records its checksum in the release manifest. Both artifacts are uploaded and verified before the prerelease becomes public. Install the matching [Linux host](linux-desktop.md) before updating Android. Local `--build-only` runs on Linux; macOS development can use the Gradle debug tasks and launch trusted CI for a distribution.

## Local preflight

Check source and version without network writes:

```bash
python3 android/scripts/release-alpha.py --check-only
```

Build verified local assets without creating a tag or publishing:

```bash
export KITE_ALPHA_KEYSTORE="$HOME/.config/kite/signing/android-alpha.keystore"
python3 android/scripts/release-alpha.py --build-only
```

The local build also accepts `KITE_ALPHA_STORE_PASSWORD`, `KITE_ALPHA_KEY_PASSWORD`, `KITE_ALPHA_KEY_ALIAS` and `KITE_EXPECTED_CERTIFICATE_SHA256`. SDK, JDK and Rust requirements are in [Development](development.md). Assets go to the ignored `android/release-output/<tag>/` directory. A local artifact is not a completed user delivery until its corresponding Alpha is published.
