//! Validated local desktop commands; Bun owns only X11/platform mechanics.
use super::*;
use serde::{Deserialize, Serialize};

pub const MAX_IMAGE: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    #[default]
    Left,
    Middle,
    Right,
}
fn window_limit() -> usize {
    16
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status,
    Windows {
        #[serde(default = "window_limit")]
        limit: usize,
    },
    Screenshot {
        #[serde(default)]
        region: Option<Region>,
        #[serde(default)]
        known_sha256: Option<String>,
    },
    Move {
        x: u32,
        y: u32,
    },
    Click {
        x: u32,
        y: u32,
        #[serde(default)]
        button: Button,
    },
    Scroll {
        dx: i32,
        dy: i32,
        #[serde(default)]
        x: Option<u32>,
        #[serde(default)]
        y: Option<u32>,
    },
    Key {
        keys: Vec<String>,
    },
    Type {
        text: String,
    },
    Launch {
        argv: Vec<String>,
        #[serde(default)]
        cwd: Option<PathBuf>,
    },
}
impl Request {
    fn validate(&self) -> Result<()> {
        let valid = match self {
            Self::Windows { limit } => (1..=64).contains(limit),
            Self::Screenshot {
                region,
                known_sha256,
            } => {
                region.as_ref().is_none_or(|r| {
                    r.width > 0
                        && r.height > 0
                        && u64::from(r.width) * u64::from(r.height) <= 16_777_216
                }) && known_sha256
                    .as_ref()
                    .is_none_or(|sha| sha.len() == 64 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
            }
            Self::Scroll { dx, dy, x, y } => {
                (-50..=50).contains(dx) && (-50..=50).contains(dy) && x.is_some() == y.is_some()
            }
            Self::Key { keys } => {
                !keys.is_empty()
                    && keys.len() <= 16
                    && keys.iter().all(|key| {
                        !key.is_empty()
                            && key.len() <= 128
                            && key.split('+').all(|symbol| {
                                !symbol.is_empty()
                                    && symbol
                                        .bytes()
                                        .all(|c| c.is_ascii_alphanumeric() || c == b'_')
                            })
                    })
            }
            Self::Type { text } => {
                text.chars().count() <= 512 && text.len() <= 2048 && !text.contains('\0')
            }
            Self::Launch { argv, cwd } => {
                !argv.is_empty()
                    && argv.len() <= 32
                    && argv.iter().map(String::len).sum::<usize>() <= 8192
                    && argv.iter().all(|s| !s.contains('\0'))
                    && !argv[0].is_empty()
                    && cwd.as_ref().is_none_or(|p| p.is_absolute())
            }
            _ => true,
        };
        if !valid {
            return Err(Error::Config(
                "invalid desktop parameters; run kite docs desktop/input",
            ));
        }
        Ok(())
    }
}
pub fn parse(value: Value) -> Result<Request> {
    let request: Request = serde_json::from_value(value.clone())
        .map_err(|_| Error::Config("invalid desktop request; run kite docs desktop/input"))?;
    let canonical = serde_json::to_value(&request)?;
    if value
        .as_object()
        .is_none_or(|o| o.keys().any(|k| canonical.get(k).is_none()))
    {
        return Err(Error::Config("unknown desktop parameter"));
    }
    request.validate()?;
    Ok(request)
}

pub async fn execute(config: &Config, request: Request) -> Result<Value> {
    request.validate()?;
    if config
        .environment
        .get("DISPLAY")
        .is_none_or(String::is_empty)
    {
        return if matches!(request, Request::Status) {
            Ok(json!({"available":false,"reason":"display_not_configured","backend":"x11"}))
        } else {
            Err(Error::Config("desktop display is not configured"))
        };
    }
    let directory = config
        .socket
        .parent()
        .ok_or(Error::Config("missing node directory"))?
        .join("desktop-artifacts");
    let mut command = Command::new(&config.bun);
    command
        .arg(&config.backend)
        .arg("desktop")
        .env_remove("DISPLAY")
        .env_remove("XAUTHORITY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .envs(&config.environment)
        .env("KITE_DESKTOP_ARTIFACTS", directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let value: Value = tokio::time::timeout(Duration::from_secs(25), async {
        let mut child = command.spawn()?;
        let mut input = child
            .stdin
            .take()
            .ok_or(Error::Protocol("desktop input unavailable"))?;
        input.write_all(&serde_json::to_vec(&request)?).await?;
        input.shutdown().await?;
        drop(input);
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or(Error::Protocol("desktop output unavailable"))?
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 256 * 1024 {
            return Err(Error::Protocol("desktop result too large"));
        }
        if !child.wait().await?.success() {
            return Err(Error::Protocol("desktop adapter exited"));
        }
        Ok::<Value, Error>(serde_json::from_slice(&bytes)?)
    })
    .await
    .map_err(|_| Error::Protocol("desktop action timed out; inspect before retrying"))??;
    if value["ok"] != true {
        let code = value["error"].as_str().unwrap_or("desktop_adapter_failed");
        // The adapter reports bounded codes, never command arguments or typed text.
        if matches!(request, Request::Status)
            && matches!(
                code,
                "x11_unavailable" | "wayland_session_unsupported" | "libx11_missing"
            )
        {
            return Ok(json!({"available":false,"reason":code,"backend":"x11"}));
        }
        return Err(Error::Remote(format!(
            "desktop: {}",
            code.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(80)
                .collect::<String>()
        )));
    }
    Ok(value["result"].clone())
}

pub fn image(socket: &Path, value: &Value) -> Result<Option<Vec<u8>>> {
    if value["unchanged"] == true || value.get("path").is_none() {
        return Ok(None);
    }
    let root = socket
        .parent()
        .ok_or(Error::Protocol("missing artifact root"))?
        .join("desktop-artifacts")
        .canonicalize()?;
    let file = Path::new(
        value["path"]
            .as_str()
            .ok_or(Error::Protocol("image path missing"))?,
    )
    .canonicalize()?;
    if file.parent() != Some(root.as_path())
        || !file
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("shot-"))
    {
        return Err(Error::Protocol(
            "image is outside the private artifact directory",
        ));
    }
    let metadata = fs::metadata(&file)?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE {
        return Err(Error::Protocol("invalid desktop image size"));
    }
    let bytes = fs::read(file)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::Protocol("desktop image is not PNG"));
    }
    Ok(Some(bytes))
}

pub struct Cli {
    pub request: Request,
    pub output: Option<PathBuf>,
}
pub fn cli(args: &[String]) -> Result<Cli> {
    let action = args.first().map(String::as_str).unwrap_or("status");
    let mut value = json!({"action":action});
    let number = |index: usize| -> Result<i64> {
        args.get(index)
            .and_then(|s| s.parse().ok())
            .ok_or(Error::Config(
                "desktop coordinate/count is missing or invalid",
            ))
    };
    let mut output = None;
    match action {
        "status" if args.len() <= 1 => {}
        "windows" if args.len() <= 2 => {
            if args.len() == 2 {
                value["limit"] = json!(number(1)?);
            }
        }
        "move" if args.len() == 3 => {
            value["x"] = json!(number(1)?);
            value["y"] = json!(number(2)?);
        }
        "click" if args.len() == 3 || args.len() == 4 => {
            value["x"] = json!(number(1)?);
            value["y"] = json!(number(2)?);
            if let Some(button) = args.get(3) {
                value["button"] = json!(button);
            }
        }
        "scroll" if args.len() == 3 || args.len() == 6 => {
            value["dx"] = json!(number(1)?);
            value["dy"] = json!(number(2)?);
            if args.len() == 6 {
                if args[3] != "--at" {
                    return Err(Error::Config("scroll accepts --at X Y"));
                }
                value["x"] = json!(number(4)?);
                value["y"] = json!(number(5)?);
            }
        }
        "key" => value["keys"] = json!(&args[1..]),
        "type" if args.len() == 2 && args[1] == "--stdin" => {
            use std::io::Read;
            let mut text = String::new();
            std::io::stdin().take(2049).read_to_string(&mut text)?;
            value["text"] = json!(text);
        }
        "launch" => {
            let mut start = 1;
            if args.get(start).map(String::as_str) == Some("--cwd") {
                start += 1;
                let path = args
                    .get(start)
                    .ok_or(Error::Config("launch requires a cwd"))?;
                value["cwd"] = json!(std::env::current_dir()?.join(path));
                start += 1;
            }
            if args.get(start).map(String::as_str) != Some("--") {
                return Err(Error::Config("launch requires -- followed by an argv"));
            }
            value["argv"] = json!(&args[start + 1..]);
        }
        "screenshot" => {
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--region" => {
                        value["region"] = json!({"x":number(i+1)?,"y":number(i+2)?,"width":number(i+3)?,"height":number(i+4)?});
                        i += 5;
                    }
                    "--known-sha256" => {
                        i += 1;
                        value["known_sha256"] = json!(
                            args.get(i)
                                .ok_or(Error::Config("missing screenshot hash"))?
                        );
                        i += 1;
                    }
                    "--output" => {
                        i += 1;
                        output = Some(PathBuf::from(
                            args.get(i)
                                .ok_or(Error::Config("missing screenshot destination"))?,
                        ));
                        i += 1;
                    }
                    _ => return Err(Error::Config("unknown screenshot option")),
                }
            }
        }
        _ => {
            return Err(Error::Config(
                "unknown desktop command; run kite docs desktop",
            ));
        }
    }
    Ok(Cli {
        request: parse(value)?,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_actions_and_option_injection_never_reach_the_adapter() {
        for value in [
            json!({"action":"shell","command":"bad"}),
            json!({"action":"status","command":"bad"}),
            json!({"action":"key","keys":["--window"]}),
            json!({"action":"scroll","dx":0,"dy":51}),
            json!({"action":"scroll","dx":0,"dy":1,"x":2}),
            json!({"action":"launch","argv":["app"],"cwd":"relative"}),
        ] {
            assert!(parse(value).is_err());
        }
        assert!(parse(json!({"action":"type","text":"test\n你好"})).is_ok());
    }
    #[tokio::test]
    async fn an_unconfigured_desktop_is_not_a_healthy_desktop() {
        let root = tempfile::tempdir().unwrap();
        let config = super::super::tests::config(root.path());
        assert_eq!(
            execute(&config, Request::Status).await.unwrap()["available"],
            false
        );
        assert!(
            execute(&config, Request::Move { x: 1, y: 1 })
                .await
                .is_err()
        );
    }
}
