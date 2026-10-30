# Alpha 10 APK signature checks — 2026-10-30

The handset reported Android 16, a 20.73 MB Alpha 9 download and error `-103`. Android names this error `INSTALL_PARSE_FAILED_NO_CERTIFICATES`: the installer failed to collect usable APK certificates. It identifies the failure layer but does not establish whether the handset file was altered or its installer used a different verification path.

The published Alpha 9 attachment is 20,727,996 bytes with SHA-256 `d3e326ebbdbc968f833d492f77a7f884c98a69037f9b6bcce42f261127f3b529`. ZIP CRC, APK v2 verification for APIs 26–36, 16 KiB ZIP alignment and native ELF alignment all pass. It has no JAR/v1 certificate entries. This does not by itself make a v2-signed APK invalid on Android 16: the unmodified Alpha 9 attachment also installed through the AOSP Android 16 system installer in this validation.

Alpha 10 explicitly includes v1, v2 and v3 signatures. Modern verification covers v2/v3; a separate API-23 verifier pass exercises JAR/v1. All use the existing certificate SHA-256 `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`. The release gate rejects a missing scheme, a different certificate between verification paths, or invalid 16 KiB ZIP alignment. The app remains minSdk 26 / targetSdk 36.

The candidate Release APK passed an actual **Alpha 9 → Alpha 10 update** through Android 16 Files / Download Manager and the stock PackageInstaller UI, displaying [App installed](system-installer-success.png). The first-install timestamp was retained and the version advanced from code 11 to 12. No app uninstall or data reset was used. Release lint, eleven Android unit tests and four publisher tests passed. No Android application, backend protocol or stored identity behavior changed in this release.

The reported physical NX769J installer is not connected to this validation environment, so its exact `-103` failure and success with Alpha 10 remain unconfirmed. Emulator installation and signature checks are not presented as handset acceptance.

Sources: [Android error definition](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/content/pm/PackageManager.java), [apksigner verification](https://developer.android.com/tools/apksigner), [AGP 9.4 SigningConfig](https://developer.android.com/reference/tools/gradle-api/9.4/com/android/build/api/variant/SigningConfig).
