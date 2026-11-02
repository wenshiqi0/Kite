use super::*;

impl AppCore {
    pub async fn set_chat_setting(
        self: &Arc<Self>,
        id: String,
        setting: String,
        value: String,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let connection = core.connection()?;
            let agent = core
                .snapshot()
                .agents
                .into_iter()
                .find(|agent| agent.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            let key = chat_key(&connection, &agent);
            let client = core
                .chat_clients
                .lock()
                .await
                .get(&key)
                .cloned()
                .ok_or_else(|| AppError::State("chat backend unavailable".into()))?;
            composer::settings::validate(
                &client.settings,
                &BTreeMap::from([(setting.clone(), value.clone())]),
            )?;
            let selected = {
                let store = lock(&core.store);
                let store = store
                    .as_ref()
                    .ok_or_else(|| AppError::State("store unavailable".into()))?;
                let mut settings: BTreeMap<String, Selection> =
                    store.read_document("chat-settings")?.unwrap_or_default();
                let selected = settings
                    .entry(key)
                    .or_insert_with(|| client.initial.clone());
                selected.settings.insert(setting, value);
                let selected = selected.clone();
                store.write_document("chat-settings", &settings)?;
                selected
            };
            if core.generation.load(Ordering::SeqCst) == connection.generation {
                core.update(|snapshot| {
                    if snapshot.selected_agent.as_ref() == Some(&id) {
                        snapshot.composer.settings =
                            composer::settings::display(&client.settings, &selected.settings);
                        snapshot.composer.selection = selected;
                    }
                });
            }
            Ok(())
        })
        .await
    }
}
