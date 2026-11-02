# Alpha 65 installer rebuild

The user reports another installation error `-103` for Alpha 64 and requests a rebuilt package. Their first reference to Alpha 65 was corrected to Alpha 64 before packaging. The failed handset file and detailed installer exception are unavailable; no specific verifier defect or certificate-expiry cause is established.

## Alpha 64 public baseline

All five public Alpha 64 assets were downloaded again. Their sizes, GitHub asset digests and published SHA256SUMS match. SDK 37 verifies the original certificate, v1 on API 23, v3 on API 36 for the primary APK, and v2 on API 36 for the compatibility APK. A second verifier, Debian apksigner 35.0.2 on Temurin JRE 21.0.13, independently passes both API paths. Both APKs have identical non-signing entries and pass 16 KiB native ZIP alignment. The host archive identifies the same source commit. [Exact checks](baseline.json).

These public-byte checks do not reproduce the phone failure. Android defines `-103` as `INSTALL_PARSE_FAILED_NO_CERTIFICATES`; it identifies certificate collection as the failure layer without identifying the underlying exception. [Android definition](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/content/pm/PackageManager.java).

## Rebuild scope

Alpha 65 increments the release version to `0.2.0-alpha.65` / versionCode **67**, preserving Alpha 64 application and host implementation sources. The existing trusted tagged CI rebuilds the Release APK, aligns it before final SDK signing and retains the original certificate. The primary asset keeps v1/v2/v3; the explicitly recommended `-compat-v2.apk` keeps v1/v2 and disables v3/v4. Publication creates a new immutable prerelease and download URL, retaining earlier releases. [Signing order and verification](https://developer.android.com/tools/apksigner).

The nine existing publisher regression tests and source whitespace checks pass before tagging. No phone installation is claimed, and this packaging-only release does not require restarting or replacing the running Alpha 64 host.

## Published Alpha 65

[Alpha 65](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.65), versionCode **67**, is published from `9e6f72625146739982208f08d74c30339ae06cda` as a prerelease. Both [main CI](https://github.com/wenshiqi0/Kite/actions/runs/41423110596) and [tagged build/publication](https://github.com/wenshiqi0/Kite/actions/runs/41423111067) pass.

All five public assets were freshly downloaded after publication. Published checksums, GitHub asset digests, source/version metadata, original signing certificate, API-23 v1, API-36 primary v3 / compatibility v2 and 16 KiB native alignment pass. The separate SDK 35 verifier also passes both API paths. Primary and compatibility application entries are identical. [Public verification report](published-release.json).

Comparing the public compatibility APKs for Alpha 64 and 65 finds no added or removed non-signing ZIP entries; only `AndroidManifest.xml` differs. All DEX, resources and native libraries are byte-identical. [Payload comparison](payload-comparison.json).

Recommended [direct compatibility download](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.65/kite-0.2.0-alpha.65-arm64-v8a-compat-v2.apk). Physical handset installation and confirmation of whether this rebuild resolves its `-103` remain pending the user's installation result.
