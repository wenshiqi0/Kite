# Alpha 78 installer rebuild

The user reports `-103` when opening the supplied Alpha 77 compatibility APK directly from the browser and explicitly requests the same fresh rebuild used for earlier occurrences. Rebuild the installer with the existing certificate, a new Alpha version/versionCode and a new immutable Release URL; retain the JSON report presentation fix.

The public Alpha 77 compatibility file is 25,431,136 bytes with SHA-256 `fb04b604bdd91edc003e8a9f0e85a6b0aaa0383746990635568bfcd77adeec34`. ZIP CRC and duplicate-entry checks pass. SDK 37 and independent Debian SDK 35 both pass forced API 23/v1 and API 36/v2 verification with the established certificate `f0a8e6b2e38c0679b39961c3f4d04d97c171b8aba22376daeb5ebd8a7e701273`.

An isolated Android 16/API 36 emulator installed the public Alpha 66 compatibility APK and then cover-installed the exact public Alpha 77 compatibility APK. The Package Manager returned Success, retained firstInstallTime and advanced to versionCode 79. The disposable emulator was stopped. These are controlled baseline results, not a reproduction of the physical phone's exception.

Alpha 78 follows the existing trusted publisher: build, ZIP-align, explicitly re-sign primary and compatibility APKs, verify certificate/version/signatures/payload, then publish and re-download immutable assets for checksum verification. Application sources are unchanged from Alpha 77. No uninstall, data reset, key replacement or host restart is part of this rebuild.
