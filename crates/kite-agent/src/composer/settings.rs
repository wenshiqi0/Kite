//! Advertised session controls, validated before any native permission override.
use super::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SettingChoice {
    pub value: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSetting {
    pub id: String,
    pub name: String,
    pub current_value: String,
    pub options: Vec<SettingChoice>,
}

pub fn validate(
    settings: &[SessionSetting],
    values: &BTreeMap<String, String>,
) -> Result<(), Error> {
    if values.iter().any(|(id, value)| {
        !settings.iter().any(|setting| {
            &setting.id == id && setting.options.iter().any(|option| &option.value == value)
        })
    }) {
        return Err(Error::Invalid("session permission choice unavailable"));
    }
    Ok(())
}

pub fn display(
    settings: &[SessionSetting],
    values: &BTreeMap<String, String>,
) -> Vec<SessionSetting> {
    settings
        .iter()
        .cloned()
        .map(|mut setting| {
            if let Some(value) = values
                .get(&setting.id)
                .filter(|value| setting.options.iter().any(|option| &option.value == *value))
            {
                setting.current_value = value.clone();
            }
            setting
        })
        .collect()
}

pub fn decode_native(value: &Value) -> Result<Vec<SessionSetting>, Error> {
    let settings: Vec<SessionSetting> = serde_json::from_value(value.clone())?;
    let mut ids = std::collections::BTreeSet::new();
    if settings.len() > 8
        || settings.iter().any(|setting| {
            setting.id.is_empty()
                || !ids.insert(&setting.id)
                || setting.options.is_empty()
                || setting.options.len() > 32
                || !setting
                    .options
                    .iter()
                    .any(|option| option.value == setting.current_value)
        })
    {
        return Err(Error::Invalid("invalid session permission settings"));
    }
    Ok(settings)
}

pub fn codex(config: &Value, requirements: &Value) -> Vec<SessionSetting> {
    [
        (
            "approval_policy",
            "Tool approval",
            "allowedApprovalPolicies",
            &["untrusted", "on-request", "never"][..],
        ),
        (
            "sandbox_mode",
            "File access",
            "allowedSandboxModes",
            &["read-only", "workspace-write", "danger-full-access"][..],
        ),
    ]
    .into_iter()
    .filter_map(|(id, name, allowed_key, known)| {
        let current = config[id].as_str().unwrap_or("default");
        let allowed = requirements[allowed_key].as_array();
        let mut options = vec![SettingChoice {
            value: "default".into(),
            name: "Use current host setting".into(),
        }];
        options.extend(
            known
                .iter()
                .filter(|value| {
                    allowed
                        .is_none_or(|items| items.iter().any(|item| item.as_str() == Some(**value)))
                })
                .map(|value| SettingChoice {
                    value: (*value).into(),
                    name: (*value).into(),
                }),
        );
        if options.len() == 1 {
            return None;
        }
        let current_value = if options.iter().any(|option| option.value == current) {
            current
        } else {
            "default"
        };
        Some(SessionSetting {
            id: id.into(),
            name: name.into(),
            current_value: current_value.into(),
            options,
        })
    })
    .collect()
}

pub fn apply(params: &mut Value, values: &BTreeMap<String, String>, managed: bool, config: &Value) {
    if managed {
        params["sessionSettings"] = json!(values);
        return;
    }
    if let Some(value) = values
        .get("approval_policy")
        .filter(|value| value.as_str() != "default")
    {
        params["approvalPolicy"] = json!(value);
    }
    if let Some(value) = values
        .get("sandbox_mode")
        .filter(|value| value.as_str() != "default")
    {
        params["sandboxPolicy"] = match value.as_str() {
            "read-only" => json!({"type":"readOnly"}),
            "workspace-write" => json!({
                "type":"workspaceWrite",
                "writableRoots":config["sandbox_workspace_write"]["writable_roots"].as_array().cloned().unwrap_or_default(),
                "networkAccess":config["sandbox_workspace_write"]["network_access"].as_bool().unwrap_or(false),
                "excludeTmpdirEnvVar":config["sandbox_workspace_write"]["exclude_tmpdir_env_var"].as_bool().unwrap_or(false),
                "excludeSlashTmp":config["sandbox_workspace_write"]["exclude_slash_tmp"].as_bool().unwrap_or(false),
            }),
            "danger-full-access" => json!({"type":"dangerFullAccess"}),
            _ => unreachable!("settings validated before native request"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admin_restrictions_and_actual_native_values_govern_overrides() {
        let config = json!({"approval_policy":"on-request","sandbox_mode":"workspace-write","sandbox_workspace_write":{"writable_roots":["/project"],"network_access":false}});
        let settings = codex(
            &config,
            &json!({"allowedApprovalPolicies":["on-request"],"allowedSandboxModes":["read-only","workspace-write"]}),
        );
        assert!(
            validate(
                &settings,
                &BTreeMap::from([("approval_policy".into(), "never".into())])
            )
            .is_err()
        );
        assert!(
            validate(
                &settings,
                &BTreeMap::from([("sandbox_mode".into(), "danger-full-access".into())])
            )
            .is_err()
        );
        let values = BTreeMap::from([
            ("approval_policy".into(), "on-request".into()),
            ("sandbox_mode".into(), "workspace-write".into()),
        ]);
        validate(&settings, &values).unwrap();
        let mut params = json!({});
        apply(&mut params, &values, false, &config);
        assert_eq!(params["approvalPolicy"], "on-request");
        assert_eq!(
            params["sandboxPolicy"]["writableRoots"],
            json!(["/project"])
        );
        assert_eq!(params["sandboxPolicy"]["networkAccess"], false);
        let mut managed = json!({});
        apply(&mut managed, &values, true, &config);
        assert_eq!(managed["sessionSettings"], json!(values));
        assert!(managed.get("approvalPolicy").is_none());
    }
}
