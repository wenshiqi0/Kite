# Alpha 33 signing compatibility test

The handset reports `-103` for the latest Alpha 32 package. The user confirms that their APK is intact and suspects signing. The handset file, installer stack trace and active signing-verifier path were not captured. Android defines `-103` as `INSTALL_PARSE_FAILED_NO_CERTIFICATES`; this establishes the error layer, not its exact cause.

Independent local verification uses Temurin JRE 21.0.13 and the Debian-distributed Android SDK apksigner 35.0.2. Public Alpha 10 (previously accepted by the handset) and Alpha 32 both verify across APIs 23–36 with v1/v2/v3, and on API 36 with v3. Their certificate is byte-identical by SHA-256: `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`. The v1 PKCS#7 envelope sizes, algorithms and certificate identity also match; signature-file producer headers differ after Alpha 27's SDK re-signing. [Baseline checks](baseline.json). These results do not establish handset installation success.

## Controlled compatibility variant

Alpha 33 adds `-compat-v2.apk` beside the existing v1/v2/v3 APK. Both are signed from the same Gradle Release payload, using the same existing certificate. The compatibility asset enables v1/v2 and explicitly disables v3/v4. It makes Android 16 verify with v2 instead of selecting the v3 path. Android supports v2 whole-file APK integrity; v3 adds key-rotation metadata, which this release does not use.

The publisher independently requires v1 verification on API 23, v2 verification on API 36 for the compatibility asset, the same certificate in each path, 16 KiB ZIP alignment, and identical non-signing payload hashes across both APKs. Both assets have the same package name, versionName and versionCode and support an in-place update. Publication checks their manifest provenance and downloads every public asset to verify hashes. No existing tag or APK is replaced.

This is an isolation test for the phone's signature-verification path, not a confirmed finding of a device v3 defect. It preserves the Alpha 32 features and existing signing identity. A handset installation result remains necessary to establish whether this variant resolves the reported failure.

Sources: [Android verifier](https://android.googlesource.com/platform/frameworks/base/+/master/core/java/android/util/apk/ApkSignatureVerifier.java), [v2 format and integrity](https://source.android.com/docs/security/features/apksigning/v2), [v3 format and rotation](https://source.android.com/docs/security/features/apksigning/v3), [SDK verifier controls](https://developer.android.com/tools/apksigner).

## Published Alpha 33

[Alpha 33 compatibility APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.33/kite-0.2.0-alpha.33-arm64-v8a-compat-v2.apk), versionCode 35, is published from `2d582141b439aa37e6b67fdbee902b7dd4cc6af0`. [Main CI](https://github.com/wenshiqi0/Kite/actions/runs/41302380248) and [tag publication](https://github.com/wenshiqi0/Kite/actions/runs/41302385318) pass. Eight publisher regressions include the v2-only configuration, API-36 verification and retained certificate/payload rules.

The downloaded compatibility asset is 23,481,440 bytes, SHA-256 `4fafd181f2125a99fee4766122451e82f29de9010ac6e9408171dba9d1147acc`. Independent verification passes v1/v2 across APIs 23–36 and selects v2 on API 36; v3/v3.1 blocks are absent. Its v1 signature advertises only scheme 2. The primary asset selects v3 on API 36. Both public APKs retain the original certificate and byte-identical non-signing entry contents, and both pass ZIP CRC/native alignment checks. [Complete public evidence](published-release.json).

No handset install was performed by the agent. Whether the compatibility asset resolves the user's `-103` remains pending the phone's installation result; the generic SDK checks must not be presented as handset acceptance.
