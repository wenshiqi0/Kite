use super::*;
use crate::resources::HostResources;

impl AppCore {
    fn install_resources(&self, host: &str, generation: u64, profile: HostResources) {
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        self.update(|state| {
            if state.selected_host.as_deref() == Some(host)
                && state.connection == ConnectionPhase::Connected
            {
                state.host_resources = Some(profile);
            }
        });
    }

    pub(super) async fn refresh_resources(&self, connection: &Connected, generation: u64) {
        let result = async {
            let output = connection
                .ssh
                .exec("\"$HOME/.local/bin/kite-node\" resources", &[])
                .await?;
            if output.exit_status != Some(0) {
                return Err(AppError::State("host resource profile unavailable".into()));
            }
            if output.stdout.len() > 256 * 1024 {
                return Err(AppError::State("host resource profile too large".into()));
            }
            serde_json::from_slice::<HostResources>(&output.stdout)
                .map_err(|e| AppError::State(e.to_string()))
        }
        .await;
        let profile = match result {
            Ok(profile) => profile,
            Err(error) => HostResources {
                problem: Some(error.to_string()),
                ..HostResources::default()
            },
        };
        self.install_resources(&connection.host.id, generation, profile);
    }

    pub async fn refresh_host_resources(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let connection = core.connection()?;
            core.refresh_resources(&connection, core.generation.load(Ordering::SeqCst))
                .await;
            Ok(())
        })
        .await
    }

    pub async fn set_agent_memory_limit(
        self: &Arc<Self>,
        host_id: String,
        bytes: u64,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _operation = core.operations.lock().await;
            let connection = core.connection()?;
            if connection.host.id != host_id {
                return Err(AppError::State(
                    "resource operation belongs to another host".into(),
                ));
            }
            let generation = core.generation.load(Ordering::SeqCst);
            let command = format!("\"$HOME/.local/bin/kite-node\" resource-limit --bytes {bytes}");
            let result = connection.ssh.exec(&command, &[]).await?;
            if result.exit_status != Some(0) {
                core.refresh_resources(&connection, generation).await;
                return Err(AppError::State(
                    "host rejected the agent memory budget".into(),
                ));
            }
            let profile: HostResources = serde_json::from_slice(&result.stdout)
                .map_err(|e| AppError::State(e.to_string()))?;
            core.install_resources(&host_id, generation, profile);
            Ok(())
        })
        .await
    }
}
