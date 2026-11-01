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

## Verified delivery — 2026-11-01

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

Alpha 17+ packages the Linux x86_64 host from the same commit as Android. Trusted CI builds `kite-node`, packages only the binary/native panel/installer/license/host manifest, and records its checksum in the release manifest. Both artifacts are uploaded and verified before the prerelease becomes public. Install the matching [Linux host](linux-desktop.md) before updating Android. Local `--build-only` runs on Linux; macOS development can use the Gradle debug tasks and launch trusted CI for a distribution.

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
