//! launchd mechanism; service policy remains in the host runtime.
use super::*;
use std::os::unix::fs::MetadataExt;

fn target(label: &str) -> Result<String> {
    if label.is_empty()
        || label.starts_with('-')
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return Err(Error::Config("invalid launchd label"));
    }
    let uid = fs::metadata(default_path(".")?)?.uid();
    Ok(format!("gui/{uid}/{label}"))
}
pub(super) async fn start(control: &Path, label: &str) -> Result<()> {
    let mut command = Command::new(control);
    command.args(["kickstart", &target(label)?]);
    bounded_output_with_timeout(command, SERVICE_TIMEOUT).await?;
    Ok(())
}
pub(super) fn parse(service: &Service, bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);
    let mut value = service_state(service);
    let mut state = None;
    for line in text.lines() {
        let line = line.trim();
        if state.is_none()
            && let Some(s) = line.strip_prefix("state = ")
        {
            state = Some(s);
        }
        if value["pid"] == 0
            && let Some(pid) = line
                .strip_prefix("pid = ")
                .and_then(|v| v.parse::<u32>().ok())
        {
            value["pid"] = json!(pid);
        }
    }
    value["active"] = json!(state == Some("running") && value["pid"].as_u64().unwrap_or(0) > 0);
    if state.is_none() {
        value["error"] = json!("launchd service state unavailable");
    }
    value
}
pub(super) async fn inspect(control: &Path, services: &[Service]) -> Vec<Value> {
    let mut tasks = JoinSet::new();
    for (index, service) in services.iter().cloned().enumerate() {
        let control = control.to_path_buf();
        tasks.spawn(async move {
            let result = async {
                let mut command = Command::new(control);
                command.args(["print", &target(&service.unit)?]);
                bounded_output_with_timeout(command, SERVICE_TIMEOUT).await
            }
            .await;
            let value = match result {
                Ok(bytes) => parse(&service, &bytes),
                Err(_) => {
                    let mut value = service_state(&service);
                    value["error"] = json!("launchd service is not running or not registered");
                    value
                }
            };
            (index, value)
        });
    }
    let mut values = BTreeMap::new();
    while let Some(Ok((index, value))) = tasks.join_next().await {
        values.insert(index, value);
    }
    values.into_values().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loaded_is_not_running_and_pid_is_required() {
        let service = Service {
            name: "Herdr".into(),
            unit: "io.github.wenshiqi0.kite.herdr".into(),
            external: false,
        };
        assert_eq!(
            parse(&service, b"state = not running\npid = 0\n")["active"],
            false
        );
        assert_eq!(
            parse(&service, b"state = running\npid = 42\n")["active"],
            true
        );
        assert_eq!(parse(&service, b"state = running\n")["active"], false);
        assert_eq!(
            parse(&service, b"state = running\npid = 42\n\t\tstate = active\n")["active"],
            true
        );
        assert!(target("--bad").is_err());
    }
}
