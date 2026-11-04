//! Network ACP CLI reuses the pinned host adapter and the host's private environment.
use super::{Config, Error, Result, default_path};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
    process::Command,
};

pub(super) fn cli(args: &[String]) -> Result<i32> {
    if args.is_empty() || args.iter().any(|value| value == "--help" || value == "-h") {
        println!(
            "Kite network ACP (v1 WebSocket profile)\n  kite acp serve --workspace PATH [--workspace PATH] [--port N] [--token-file PATH]\n  kite acp proxy --peer NAME [--peers PATH]\n  kite acp task --peer NAME --cwd PATH [--session ID] [--expected-revision SHA] < prompt.txt\n  kite acp mcp [--peers PATH]\nPeer file: ~/.config/kite/acp/peers.json; private tokens stay outside the repository."
        );
        return Ok(0);
    }
    if args.len() > 64
        || !matches!(args[0].as_str(), "serve" | "proxy" | "task" | "mcp")
        || args.iter().any(|s| s.len() > 8192 || s.contains('\0'))
    {
        return Err(Error::Config("invalid network ACP command"));
    }
    let config_path = default_path(".config/kite/node.json")?;
    let config: Option<Config> = if config_path.is_file() {
        let config: Config = serde_json::from_slice(&fs::read(&config_path)?)?;
        config.validate()?;
        Some(config.current()?)
    } else {
        None
    };
    if args[0] == "serve" && config.is_none() {
        return Err(Error::Config(
            "configure this Kite host before serving its sessions",
        ));
    }
    let source = kite_agent::composer::BACKEND_SOURCE;
    let generation =
        default_path(".local/state/kite/acp/adapters")?.join(kite_agent::fingerprint(&format!(
            "{}{}",
            String::from_utf8_lossy(source),
            String::from_utf8_lossy(kite_agent::composer::DSH_PLUGIN_SOURCE)
        )));
    fs::create_dir_all(&generation)?;
    fs::set_permissions(
        generation
            .parent()
            .ok_or(Error::Config("invalid ACP adapter path"))?,
        fs::Permissions::from_mode(0o700),
    )?;
    fs::set_permissions(&generation, fs::Permissions::from_mode(0o700))?;
    let adapter = generation.join("host.js");
    for (file, body) in [
        (&adapter, source),
        (
            &generation.join("dsh-plugin.mjs"),
            kite_agent::composer::DSH_PLUGIN_SOURCE,
        ),
    ] {
        if fs::read(file).is_ok_and(|existing| existing == body) {
            continue;
        }
        let pending = file.with_extension(format!("pending-{}", std::process::id()));
        fs::write(&pending, body)?;
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o600))?;
        fs::rename(pending, file)?;
    }
    let bun = config
        .as_ref()
        .map(|c| c.bun.clone())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join("bun"))
                    .find(|path| path.is_file())
            })
        })
        .unwrap_or_else(|| PathBuf::from("bun"));
    let mut command = Command::new(bun);
    command
        .arg(&adapter)
        .arg("acp")
        .args(args)
        .env("KITE_HOST_BACKEND_PATH", &adapter);
    if let Some(config) = config {
        command
            .envs(config.environment)
            .env("KITE_ACP_HERDR", config.herdr);
    }
    command.env(
        "KITE_AGENT_RESOURCE_CONTROL",
        if cfg!(target_os = "linux") { "1" } else { "0" },
    );
    Err(Error::Io(command.exec()))
}
