# Android Alpha releases

The GitHub Actions workflow is [Android Release](../.github/workflows/android-release.yml).

## What runs

| Event | Build | Delivery |
| --- | --- | --- |
| Relevant changes on `main` | Signed Release APK, Rust checks and Android lint | Verified workflow artifact |
| Pull request to `main` | Unsigned Release APK, Rust checks and Android lint | Unsigned PR artifact; no signing secrets |
| Annotated `vX.Y.Z-alpha.N` tag | Signed Release APK with the tag's source commit | GitHub prerelease with APK, checksums and manifest |
| Manual workflow dispatch | Signed Release APK | Verified workflow artifact |

Only tagged builds publish a user-facing GitHub Release. They are always prereleases and are never marked as the latest stable release. Workflow artifacts are retained for 14 days, or 7 days for unsigned PR builds.

## Version and signing identity

`android/version.properties` is the Android version source. Increment both the Alpha suffix and `versionCode` for each new delivery. The initial CI release is `0.2.0-alpha.1` / `3`. APK metadata must match the tag exactly.

Alpha uses the same signing identity as the earlier test APKs so existing installations can upgrade without replacing their device SSH key, hosts or trust records. The keystore remains outside Git. CI receives it through an encrypted Actions secret; it is never an artifact.

Alpha 10 explicitly enables APK signing schemes v1, v2 and v3 with this same key. The publisher requires all three schemes to verify and checks 16 KiB ZIP alignment before creating the release manifest. These checks cover the reported `-103` certificate-collection failure path; a handset's downloaded bytes and installer behavior still require device verification. Signature schemes and ZIP alignment are recorded in the manifest.

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

The build verifies the APK's version, non-debuggable Release variant, ABI, signature and pinned certificate. The publish job checks the source commit and checksums, uploads a draft prerelease, downloads every asset to verify its bytes, and then publishes it. An existing tag cannot move to another commit, and existing release assets cannot be replaced with different bytes.

If a publish step is interrupted, rerun that failed job from GitHub Actions so it reuses the same verified build artifact. If source or artifact content changes, use a new Alpha version.

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
