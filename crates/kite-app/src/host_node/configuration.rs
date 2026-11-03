//! Shared host configuration and the service launchers that consume it.
use super::*;
use std::os::unix::process::CommandExt;

pub(super) const DESKTOP_ENVIRONMENT: &[&str] = &[
    "DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
];
const PUBLIC_SETTINGS: &[&str] = &[
    "capture",
    "encoder",
    "adapter_name",
    "output_name",
    "address_family",
    "bind_address",
    "port",
    "upnp",
    "lan_encryption_mode",
    "wan_encryption_mode",
    "file_apps",
    "file_state",
    "credentials_file",
    "cert",
    "pkey",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sunshine {
    pub executable: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub address: Option<String>,
    pub settings: BTreeMap<String, String>,
}

impl Sunshine {
    pub fn port(&self) -> Result<u16> {
        self.settings.get("port").map_or(Ok(47989), |value| {
            value
                .parse()
                .map_err(|_| Error::Config("invalid Sunshine port"))
        })
    }

    pub fn validate(&self) -> Result<()> {
        if !self.executable.is_absolute()
            || self.settings.contains_key("sunshine_name")
            || !(6..=65514).contains(&self.port()?)
            || self
                .args
                .iter()
                .any(|arg| arg.contains('\0') || arg.contains('='))
            || self
                .address
                .as_ref()
                .is_some_and(|address| address.is_empty() || address.chars().any(char::is_control))
            || self.settings.iter().any(|(key, value)| {
                key.is_empty()
                    || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    || value.contains('\0')
            })
        {
            return Err(Error::Config("invalid unified Sunshine configuration"));
        }
        Ok(())
    }
}

impl Config {
    pub(super) fn current(&self) -> Result<Self> {
        let Some(path) = &self.source_path else {
            return Ok(self.clone());
        };
        let mut current: Self = serde_json::from_slice(&fs::read(path)?)?;
        current.source_path = self.source_path.clone();
        current.validate()?;
        if current.node_id != self.node_id || current.socket != self.socket {
            return Err(Error::Config(
                "node identity or socket changed; restart the node",
            ));
        }
        // Existing connections retain the adapter belonging to their generation.
        current.backend = self.backend.clone();
        Ok(current)
    }

    pub(super) fn machine_name(&self) -> Result<String> {
        if !self.machine.is_empty() {
            return Ok(self.machine.clone());
        }
        let name = if cfg!(target_os = "linux") {
            fs::read_to_string("/proc/sys/kernel/hostname")?
        } else {
            let output = std::process::Command::new("/bin/hostname").output()?;
            if !output.status.success() {
                return Err(Error::Config("system hostname unavailable"));
            }
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        let name = name.trim();
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(Error::Config("invalid system hostname"));
        }
        Ok(name.into())
    }

    pub(super) fn public_configuration(&self) -> Result<Value> {
        let desktop: BTreeMap<_, _> = self
            .environment
            .iter()
            .filter(|(key, _)| DESKTOP_ENVIRONMENT.contains(&key.as_str()))
            .collect();
        let sunshine = self
            .sunshine
            .as_ref()
            .map(|sunshine| {
                let settings: BTreeMap<_, _> = sunshine
                    .settings
                    .iter()
                    .filter(|(key, _)| PUBLIC_SETTINGS.contains(&key.as_str()))
                    .collect();
                Ok::<_, Error>(json!({
                    "executable":sunshine.executable,
                    "address":sunshine.address,
                    "port":sunshine.port()?,
                    "name":self.machine_name()?,
                    "settings":settings,
                }))
            })
            .transpose()?;
        Ok(json!({
            "version":self.configuration_version,
            "source":self.source_path,
            "machine":self.machine_name()?,
            "machineSource":if self.machine.is_empty() {"system-hostname"} else {"configured"},
            "socket":self.socket,
            "desktop":desktop,
            "herdr":{"executable":self.herdr},
            "sunshine":sunshine,
            "services":self.services,
        }))
    }
}

pub(super) fn service_exec(args: &[String]) -> Result<i32> {
    if args.len() != 3 || args[1] != "--config" {
        return Err(Error::Config(
            "service-exec requires Herdr|Sunshine --config PATH",
        ));
    }
    let mut config: Config = serde_json::from_slice(&fs::read(&args[2])?)?;
    config.source_path = Some(fs::canonicalize(&args[2])?);
    config.validate()?;
    let mut command = match args[0].as_str() {
        "Herdr" => {
            let mut command = std::process::Command::new(&config.herdr);
            command.arg("server");
            command
        }
        "Sunshine" => {
            let sunshine = config
                .sunshine
                .as_ref()
                .ok_or(Error::Config("Sunshine is not configured in Kite"))?;
            let mut command = std::process::Command::new(&sunshine.executable);
            // Sunshine accepts key=value options. An empty base prevents its
            // default config file from becoming a second source of settings.
            command.arg("/dev/null").args(&sunshine.args);
            for (key, value) in &sunshine.settings {
                if key != "sunshine_name" {
                    command.arg(format!("{key}={value}"));
                }
            }
            command.arg(format!("sunshine_name={}", config.machine_name()?));
            command
        }
        _ => return Err(Error::Config("unknown configured service")),
    };
    // A unit's old desktop snapshot must never override the shared configuration.
    for key in DESKTOP_ENVIRONMENT {
        command.env_remove(key);
    }
    if args[0] == "Herdr" {
        command.envs(&config.environment);
    } else {
        for (key, value) in &config.environment {
            if DESKTOP_ENVIRONMENT.contains(&key.as_str()) || key == "PATH" {
                command.env(key, value);
            }
        }
    }
    Err(command.exec().into())
}

pub(super) async fn probe(config: &Config) -> Value {
    let desktop = async {
        if !config.environment.contains_key("DISPLAY") {
            return json!({"available":false,"reason":"display_not_configured"});
        }
        match desktop::execute(config, desktop::Request::Status).await {
            Ok(value) => value,
            Err(_) => json!({"available":false,"reason":"desktop_probe_failed"}),
        }
    };
    let sunshine = async {
        match config.sunshine.as_ref() {
            Some(sunshine) => match sunshine_info(sunshine).await {
                Ok(value) => value,
                Err(_) => json!({"available":false,"reason":"sunshine_endpoint_unavailable"}),
            },
            None => json!({"available":false,"reason":"not_managed"}),
        }
    };
    let (desktop, sunshine) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(2), desktop),
        tokio::time::timeout(Duration::from_secs(2), sunshine),
    );
    json!({
        "desktop":desktop.unwrap_or_else(|_| json!({"available":false,"reason":"desktop_probe_timeout"})),
        "sunshine":sunshine.unwrap_or_else(|_| json!({"available":false,"reason":"sunshine_probe_timeout"})),
    })
}

async fn sunshine_info(sunshine: &Sunshine) -> Result<Value> {
    let address = match sunshine.settings.get("bind_address").map(String::as_str) {
        None | Some("0.0.0.0" | "") => "127.0.0.1",
        Some("::") => "::1",
        Some(address) => address,
    };
    let mut stream = tokio::net::TcpStream::connect((address, sunshine.port()?)).await?;
    stream
        .write_all(b"GET /serverinfo HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut bytes = Vec::new();
    stream.take(65537).read_to_end(&mut bytes).await?;
    if bytes.len() > 65536 {
        return Err(Error::Protocol("Sunshine response too large"));
    }
    let text = String::from_utf8_lossy(&bytes);
    let body = text
        .split_once("\r\n\r\n")
        .ok_or(Error::Protocol("invalid Sunshine HTTP response"))?
        .1;
    let xml = roxmltree::Document::parse(body)
        .map_err(|_| Error::Protocol("invalid Sunshine response"))?;
    if xml.root_element().attribute("status_code") != Some("200") {
        return Err(Error::Protocol("Sunshine endpoint rejected probe"));
    }
    let field = |name| {
        xml.descendants()
            .find(|node| node.has_tag_name(name))
            .and_then(|node| node.text())
    };
    Ok(
        json!({"available":true,"port":sunshine.port()?,"name":field("hostname"),"state":field("state")}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_the_current_source_and_omits_provider_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = super::super::tests::config(directory.path());
        let source = directory.path().join("node.json");
        config.source_path = Some(source.clone());
        config.environment.insert("DISPLAY".into(), ":10".into());
        config
            .environment
            .insert("MIMO_API_KEY".into(), "private-value".into());
        fs::write(&source, serde_json::to_vec(&config).unwrap()).unwrap();
        let before = config.current().unwrap().public_configuration().unwrap();
        assert_eq!(before["desktop"]["DISPLAY"], ":10");
        assert!(!before.to_string().contains("private-value"));
        let mut edited = config.clone();
        edited.environment.insert("DISPLAY".into(), ":20".into());
        edited.machine = "renamed-host".into();
        fs::write(&source, serde_json::to_vec(&edited).unwrap()).unwrap();
        let after = config.current().unwrap().public_configuration().unwrap();
        assert_eq!(after["desktop"]["DISPLAY"], ":20");
        assert_eq!(after["machine"], "renamed-host");
        edited.node_id = "different-node".into();
        fs::write(&source, serde_json::to_vec(&edited).unwrap()).unwrap();
        assert!(config.current().is_err());
    }

    #[tokio::test]
    async fn sunshine_probe_uses_the_configured_port_and_checks_the_real_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            loop {
                let mut line = String::new();
                assert!(stream.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            stream.get_mut().write_all(b"HTTP/1.0 200 OK\r\n\r\n<root status_code=\"200\"><hostname>renamed-host</hostname><state>SUNSHINE_SERVER_FREE</state></root>").await.unwrap();
        });
        let sunshine = Sunshine {
            executable: "/bin/true".into(),
            args: vec![],
            address: None,
            settings: BTreeMap::from([("port".into(), port.to_string())]),
        };
        sunshine.validate().unwrap();
        let response = sunshine_info(&sunshine).await.unwrap();
        assert_eq!(response["name"], "renamed-host");
        assert_eq!(response["port"], port);
        server.await.unwrap();
    }
}
