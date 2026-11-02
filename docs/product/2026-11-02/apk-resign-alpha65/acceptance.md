# Alpha 65 installer rebuild

The user reports another installation error `-103` for Alpha 64 and requests a rebuilt package. Their first reference to Alpha 65 was corrected to Alpha 64 before packaging. The failed handset file and detailed installer exception are unavailable; no specific verifier defect or certificate-expiry cause is established.

## Alpha 64 public baseline

All five public Alpha 64 assets were downloaded again. Their sizes, GitHub asset digests and published SHA256SUMS match. SDK 37 verifies the original certificate, v1 on API 23, v3 on API 36 for the primary APK, and v2 on API 36 for the compatibility APK. A second verifier, Debian apksigner 35.0.2 on Temurin JRE 21.0.13, independently passes both API paths. Both APKs have identical non-signing entries and pass 16 KiB native ZIP alignment. The host archive identifies the same source commit. [Exact checks](baseline.json).

These public-byte checks do not reproduce the phone failure. Android defines `-103` as `INSTALL_PARSE_FAILED_NO_CERTIFICATES`; it identifies certificate collection as the failure layer without identifying the underlying exception. [Android definition](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/content/pm/PackageManager.java).

## Rebuild scope

Alpha 65 increments the release version to `0.2.0-alpha.65` / versionCode **67**, preserving Alpha 64 application and host implementation sources. The existing trusted tagged CI rebuilds the Release APK, aligns it before final SDK signing and retains the original certificate. The primary asset keeps v1/v2/v3; the explicitly recommended `-compat-v2.apk` keeps v1/v2 and disables v3/v4. Publication creates a new immutable prerelease and download URL, retaining earlier releases. [Signing order and verification](https://developer.android.com/tools/apksigner).

The nine existing publisher regression tests and source whitespace checks pass before tagging. Tagged CI and fresh public-asset verification must complete before delivery. No phone installation is claimed, and this packaging-only release does not require restarting or replacing the running Alpha 64 host.
