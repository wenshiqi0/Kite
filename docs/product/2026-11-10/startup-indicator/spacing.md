# Hidden status spacing

After Alpha 89, the user reported that the area below the title still appeared to reserve space and requested height only while content is visible.

The initialization indicator is already conditional and contributes no layout node when initialized. `WorkspaceConnectionNotice` also returns without a node while connected. A separate connected-state wrapper around `ResourceWarning` always retained 4 dp top and 4 dp bottom padding, even when `ResourceWarning` returned early for a missing or healthy profile.

Alpha 90 removes that wrapper and passes the padding as a `Modifier` to `ResourceWarning`. The modifier is applied to the warning surface after its visibility check. Missing/healthy warnings therefore add zero height, while visible warning spacing and actions are preserved. The initialization fix from Alpha 89 remains included.

Validation: visibility and padding ownership were reviewed. [Alpha 90](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.90), versionCode 92, is published from source `d20e30d204704a62d2aef314b2a6b560f6704bbb`. Both jobs in [tagged CI](https://github.com/wenshiqi0/Kite/actions/runs/42739248877) pass, including the existing Release JVM tests, lint, native test-source compilation and package checks.

Fresh public downloads of both APKs match SHA256SUMS and the manifest. Independent SDK checks confirm the previous certificate, Release variant/version, arm64 ABI, API-23 v1 signatures, API-36 primary v3 and compatibility v2 signatures, matching application payloads and 16 KiB alignment. See [published package evidence](spacing-release.json). Physical-handset layout execution remains separate.
