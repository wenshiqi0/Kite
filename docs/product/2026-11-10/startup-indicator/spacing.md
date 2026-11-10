# Hidden status spacing

After Alpha 89, the user reported that the area below the title still appeared to reserve space and requested height only while content is visible.

The initialization indicator is already conditional and contributes no layout node when initialized. `WorkspaceConnectionNotice` also returns without a node while connected. A separate connected-state wrapper around `ResourceWarning` always retained 4 dp top and 4 dp bottom padding, even when `ResourceWarning` returned early for a missing or healthy profile.

Alpha 90 removes that wrapper and passes the padding as a `Modifier` to `ResourceWarning`. The modifier is applied to the warning surface after its visibility check. Missing/healthy warnings therefore add zero height, while visible warning spacing and actions are preserved. The initialization fix from Alpha 89 remains included.

Validation: review the visibility and padding ownership and run the existing required release checks. Tagged CI and public-package verification are recorded after publication. Physical-handset layout execution remains separate.
