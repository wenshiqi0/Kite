# APK re-signing test — Alpha 27

The Android 16 handset again reported installer error `-103` for Alpha 26 and requested a freshly signed test APK. `-103` is Android's certificate-collection failure code; the exact handset file and detailed platform exception remain unavailable, so the failure must not be attributed to certificate expiry or another specific cause without evidence.

The complete public Alpha 26 attachment has 21,672,379 bytes and SHA-256 `c778618162661a8f19d897b82dcbf43656bba6b87666dd5cba9e39c13c836f4c`. Its ZIP CRC and previously recorded v1/v2/v3 verification pass. Its certificate is the same identity as handset-accepted Alpha 10: SHA-256 `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`, valid from 2026-10-28 19:36:25 +08:00 through **2054-03-15 19:36:25 +08:00**. Both Alpha 10 and 26 contain `META-INF/CERT.SF` / `CERT.RSA` with the AGP-created signature format. [Baseline](baseline.json).

Alpha 27 is a signing/packaging test. Trusted CI aligns the Gradle-built Release APK and explicitly re-signs it with Android SDK `apksigner`, using the existing key and certificate. V1/v2/v3 are explicitly enabled and v4 is disabled. Passwords are passed only in the signing subprocess environment. A ZIP/payload guard requires every non-signing entry—including native libraries and dependency metadata—to remain byte-identical through re-signing. The output is then independently checked for version, certificate, all three schemes and 16KiB alignment before publication. It receives a new Alpha number, versionCode and immutable download URL.

Application behavior, stored history, SSH identity/trust, pairing and host API 1 / backend protocol 5 are unchanged. The installed Alpha 26 backend remains compatible with this signing-only test; the matching Linux archive is also built from the same source. No new signing identity, uninstall or data reset is required.

Publisher tests cover the explicit signing command, environment-only passwords and payload scope. Final public-byte and Android 16 system-installer upgrade evidence is recorded after publication. Physical NX769J installation still needs user confirmation; emulator success cannot establish the handset's result.

Sources: [Android error definition](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/content/pm/PackageManager.java), [SDK signing and alignment order](https://developer.android.com/tools/apksigner), [signing identity and updates](https://developer.android.com/studio/publish/app-signing).

## Published test and system-installer result

The immutable [Alpha 27 test APK](https://github.com/wenshiqi0/Kite/releases/download/v0.2.0-alpha.27/kite-0.2.0-alpha.27-arm64-v8a.apk) is 21,665,635 bytes with SHA-256 `e1aa697e89d46f48ee145a096e847ea191a6b1aea88ab40f27843bcbc71e8828`, versionCode 29, from `87e41cffbffe6133198d8544bb8d5383eee94b10`. Main/tag CI passed, including six publisher tests, 95 Rust tests and Release JVM/lint. Its manifest confirms the explicit SDK re-signing step and unchanged application payload through signing. Public bytes independently pass the original certificate, v1/v2/v3 and 16KiB alignment checks. The new v1 signature reports `Created-By: 1.0 (Android)`, whereas the prior AGP signature reported `Android Gradle 9.4.1`. This confirms a new signing-tool pass, not a new certificate. [Publication verification](published-release.json).

The exact public APK was copied into the owned Android 16 emulator's Downloads folder, opened from Files and installed by the AOSP PackageInstaller's **Update** action. It displayed **App installed.** First-install time was retained; version advanced from Alpha 26/code 28 to Alpha 27/code 29. All 30 captured private files remained byte-identical before launch. No uninstall, key recreation or data reset was used. [Installer evidence](installer.json), [system-installer screenshot](system-installer-success.png).

The downloaded test copy/UI dump were removed, the owned emulator stopped and its updated app/data retained. Five pre-existing primary-checkout modifications are unchanged. No backend settings or processes were changed for this signing-only test. [Cleanup](cleanup.json).

This is emulator system-installer acceptance, not a reproduced or fixed NX769J result. The handset's exact failed bytes and certificate-collection exception are still unknown; whether the fresh test package resolves its `-103` requires the user's installation result.
