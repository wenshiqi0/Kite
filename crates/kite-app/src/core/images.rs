//! Generated-image transport; payloads never enter snapshots or transcript deltas.
use super::*;

impl AppCore {
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
                || !snapshot.conversation.iter().any(|message| {
                    composer::split_attachments(&message.text)
                        .1
                        .iter()
                        .any(|file| {
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
