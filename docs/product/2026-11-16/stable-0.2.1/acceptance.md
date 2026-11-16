# Stable 0.2.1 publication

[Kite 0.2.1](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.1) is a published stable release and GitHub's latest release. Android versionCode is 107. The immutable package source is `98ef549dacfa88db7cd8818618066a93a7762e6b`.

The [tagged build](https://github.com/wenshiqi0/Kite/actions/runs/43795081791) passed its Android/Linux build, native Linux checks, Windows cross-build and Windows runtime checks. Its publication step failed because a downloaded artifact directory appeared as an untracked source change. [Publication recovery](https://github.com/wenshiqi0/Kite/actions/runs/43797940153) verified those successful build jobs and published the same package bytes without rebuilding or changing the tag. The manifest records both the original build and the recovery tooling commit.

Independent downloads of all six Release attachments passed:

- Every `SHA256SUMS` entry and the recorded source commit/version.
- Android's existing certificate pin, primary v1/v2/v3 signatures, compatibility v1/v2 signatures, API-23/36 verification, matching application payloads and 16 KiB alignment.
- Linux host version/source metadata and both binary checksums.
- Windows source/version, embedded dependency version and every packaged file checksum.
- GitHub stable/non-draft status and latest-release selection.

Exact checksums and results are in [published-release.json](published-release.json). This delivery did not install a phone app or replace a running host.

Future releases use **Actions → Publish Release**, with a new version and optional notes. Local tests and test builds remain part of each development stage. Publication-only recovery accepts a completed build run only after its source tag and all required platform checks match.
