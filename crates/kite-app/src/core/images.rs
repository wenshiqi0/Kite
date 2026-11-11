//! Attachment transport shared by all message authors; bytes stay out of snapshots.
use super::*;

impl AppCore {
    pub async fn chat_attachment(
        self: &Arc<Self>,
        context: String,
        id: String,
    ) -> Result<Vec<u8>, AppError> {
        let core = self.clone();
        dispatch(async move {
            let snapshot = core.snapshot();
            if snapshot.conversation_context() != context {
                return Err(AppError::State("conversation changed".into()));
            }
            let file = snapshot
                .conversation
                .iter()
                .flat_map(|m| m.attachments())
                .chain(
                    snapshot
                        .composer
                        .uploads
                        .iter()
                        .filter(|u| !u.failed)
                        .map(|u| &u.attachment),
                )
                .find(|file| {
                    file.id == id
                        && (file.path == format!("kite-image:{id}")
                            || file.path.ends_with(&format!(
                                "/.local/share/kite/uploads/{}/{}",
                                file.id, file.name
                            )))
                })
                .ok_or_else(|| {
                    AppError::State(
                        "attachment no longer belongs to the selected conversation".into(),
                    )
                })?;
            let connection = core.connection()?;
            let bytes = if file.path == format!("kite-image:{id}") {
                composer::download_image(&connection.ssh, &id).await?
            } else {
                composer::download_attachment(&connection.ssh, file).await?
            };
            if core.snapshot().conversation_context() != context {
                return Err(AppError::State(
                    "conversation changed during attachment download".into(),
                ));
            }
            Ok(bytes)
        })
        .await
    }

    pub async fn chat_image(
        self: &Arc<Self>,
        context: String,
        id: String,
    ) -> Result<Vec<u8>, AppError> {
        let core = self.clone();
        dispatch(async move {
            let snapshot = core.snapshot();
            if snapshot.conversation_context() != context
                || id.len() != 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                || !snapshot.conversation.iter().any(|item| {
                    item.attachments().any(|file| {
                        file.id == id
                            && file.path == format!("kite-image:{id}")
                            && file.mime.starts_with("image/")
                    })
                })
            {
                return Err(AppError::State(
                    "image no longer belongs to the selected conversation".into(),
                ));
            }
            let connection = core.connection()?;
            let bytes = composer::download_image(&connection.ssh, &id).await?;
            if core.snapshot().conversation_context() != context {
                return Err(AppError::State(
                    "conversation changed during image download".into(),
                ));
            }
            Ok(bytes)
        })
        .await
    }
}
