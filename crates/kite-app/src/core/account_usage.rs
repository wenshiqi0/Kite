use super::*;
use crate::ProviderAccountUsage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostAccountUsageStatus {
    Ready,
    Offline,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostAccountUsage {
    pub host_id: String,
    pub host: String,
    pub status: HostAccountUsageStatus,
    pub providers: Vec<ProviderAccountUsage>,
}

impl AppCore {
    pub(super) async fn read_host_account_usage(
        &self,
    ) -> Result<Vec<ProviderAccountUsage>, AppError> {
        let generation = self.generation.load(Ordering::SeqCst);
        let connection = self.connection()?;
        let output = tokio::time::timeout(
            Duration::from_secs(28),
            connection
                .ssh
                .exec("\"$HOME/.local/bin/kite-node\" account-usage", &[]),
        )
        .await
        .map_err(|_| AppError::State("host account usage timed out".into()))??;
        if self.generation.load(Ordering::SeqCst) != generation
            || output.exit_status != Some(0)
            || output.stdout.len() > 256 * 1024
        {
            return Err(AppError::State("host account usage unavailable".into()));
        }
        let profiles: Vec<ProviderAccountUsage> = serde_json::from_slice(&output.stdout)
            .map_err(|_| AppError::State("invalid host account usage".into()))?;
        if profiles.len() > 16
            || profiles.iter().any(|row| {
                row.provider.is_empty() || row.provider.len() > 64 || row.usage.buckets.len() > 64
            })
        {
            return Err(AppError::State("host account usage exceeds limits".into()));
        }
        Ok(profiles)
    }
}
