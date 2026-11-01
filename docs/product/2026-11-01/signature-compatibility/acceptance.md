# Alpha 33 signing compatibility test

The handset reports `-103` for the latest Alpha 32 package. The user confirms that their APK is intact and suspects signing. The handset file, installer stack trace and active signing-verifier path were not captured. Android defines `-103` as `INSTALL_PARSE_FAILED_NO_CERTIFICATES`; this establishes the error layer, not its exact cause.

Independent local verification uses Temurin JRE 21.0.13 and the Debian-distributed Android SDK apksigner 35.0.2. Public Alpha 10 (previously accepted by the handset) and Alpha 32 both verify across APIs 23–36 with v1/v2/v3, and on API 36 with v3. Their certificate is byte-identical by SHA-256: `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`. The v1 PKCS#7 envelope sizes, algorithms and certificate identity also match; signature-file producer headers differ after Alpha 27's SDK re-signing. [Baseline checks](baseline.json). These results do not establish handset installation success.

## Controlled compatibility variant

Alpha 33 adds `-compat-v2.apk` beside the existing v1/v2/v3 APK. Both are signed from the same Gradle Release payload, using the same existing certificate. The compatibility asset enables v1/v2 and explicitly disables v3/v4. It makes Android 16 verify with v2 instead of selecting the v3 path. Android supports v2 whole-file APK integrity; v3 adds key-rotation metadata, which this release does not use.

The publisher independently requires v1 verification on API 23, v2 verification on API 36 for the compatibility asset, the same certificate in each path, 16 KiB ZIP alignment, and identical non-signing payload hashes across both APKs. Both assets have the same package name, versionName and versionCode and support an in-place update. Publication checks their manifest provenance and downloads every public asset to verify hashes. No existing tag or APK is replaced.

This is an isolation test for the phone's signature-verification path, not a confirmed finding of a device v3 defect. It preserves the Alpha 32 features and existing signing identity. A handset installation result remains necessary to establish whether this variant resolves the reported failure.

Sources: [Android verifier](https://android.googlesource.com/platform/frameworks/base/+/master/core/java/android/util/apk/ApkSignatureVerifier.java), [v2 format and integrity](https://source.android.com/docs/security/features/apksigning/v2), [v3 format and rotation](https://source.android.com/docs/security/features/apksigning/v3), [SDK verifier controls](https://developer.android.com/tools/apksigner).
