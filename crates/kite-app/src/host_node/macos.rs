//! Mac host installation and lifecycle. Swift consumes this structured CLI.
use super::*;
use std::{
    net::{SocketAddr, TcpStream},
    os::unix::fs::{MetadataExt, symlink},
};

const NODE: &str = "io.github.wenshiqi0.kite.node";
const HERDR: &str = "io.github.wenshiqi0.kite.herdr";

fn private(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    let info = fs::symlink_metadata(path)?;
    if !info.is_dir() || info.uid() != fs::metadata(default_path(".")?)?.uid() {
        return Err(Error::Config(
            "host directory must be user-owned and not a symlink",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let pending = path.with_extension(format!("{}.pending", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(mode)
        .open(&pending)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(pending, path)?;
    Ok(())
}
fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|s| s.is_file() && s.permissions().mode() & 0o111 != 0)
}
fn search_path() -> Result<String> {
    let home = default_path("")?;
    let mut paths = vec![
        home.join(".local/lib/kite/macos/codex/node_modules/.bin"),
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        home.join(".bun/bin"),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
    ];
    if let Ok(entries) = fs::read_dir(home.join(".nvm/versions/node")) {
        let mut bins: Vec<_> = entries.flatten().map(|e| e.path().join("bin")).collect();
        bins.sort();
        bins.reverse();
        paths.extend(bins);
    }
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    paths.extend([
        PathBuf::from("/usr/bin"),
        "/bin".into(),
        "/usr/sbin".into(),
        "/sbin".into(),
    ]);
    Ok(std::env::join_paths(paths)
        .map_err(|_| Error::Config("invalid executable search path"))?
        .to_string_lossy()
        .into_owned())
}
fn find(name: &str) -> Option<PathBuf> {
    if name == "bun" {
        let sibling = std::env::current_exe().ok()?.with_file_name("bun");
        if executable(&sibling) {
            return Some(sibling);
        }
    }
    std::env::split_paths(&search_path().ok()?)
        .map(|p| p.join(name))
        .find(|p| executable(p))
}
fn sunshine() -> Option<PathBuf> {
    [
        default_path("Applications/Sunshine.app").ok()?,
        "/Applications/Sunshine.app".into(),
    ]
    .into_iter()
    .find(|p| p.join("Contents/Info.plist").is_file())
}
async fn command(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut c = Command::new(path);
    c.args(args).env("PATH", search_path()?);
    bounded_output_with_timeout(c, Duration::from_secs(8)).await
}
async fn herdr_running(path: &Path) -> bool {
    command(path, &["status", "server", "--json"])
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v["running"] == true)
}
pub(super) async fn herdr_state(path: &Path, service: &Service) -> Value {
    let mut state = service_state(service);
    state["active"] = json!(herdr_running(path).await);
    state
}
pub(super) async fn sunshine_ready() -> bool {
    let bytes = command(
        Path::new("/usr/bin/curl"),
        &[
            "--silent",
            "--fail",
            "--noproxy",
            "*",
            "--max-time",
            "2",
            "http://127.0.0.1:47989/serverinfo",
        ],
    )
    .await;
    bytes.is_ok_and(|b| b.len() < 65536 && String::from_utf8_lossy(&b).contains("<uniqueid>"))
}
async fn inspect() -> Result<Value> {
    let socket = default_path(".local/state/kite/node/host.sock")?;
    let node = rpc(&socket, NodeRequest::new("status")).await.ok();
    let herdr = find("herdr");
    let bun = find("bun");
    let hostname = command(Path::new("/usr/sbin/scutil"), &["--get", "LocalHostName"])
        .await
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| format!("{}.local", s.trim()))
        .unwrap_or_default();
    let username = command(Path::new("/usr/bin/id"), &["-un"])
        .await
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default();
    let ssh = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], 22)),
        Duration::from_millis(300),
    )
    .is_ok();
    let herdr_active = match &herdr {
        Some(path) => herdr_running(path).await,
        None => false,
    };
    let desktop = sunshine_ready().await;
    Ok(
        json!({"configured":default_path(".config/kite/node.json")?.is_file(),"hostname":hostname,"username":username.trim(),"sshPort":22,"sshAvailable":ssh,"dependencies":{"herdr":herdr,"bun":bun,"sunshine":sunshine()},"hostApplication":default_path(".local/lib/kite/macos/Kite Host.app")?,"herdrRunning":herdr_active,"desktopAvailable":desktop,"chatAvailable":node.is_some() && herdr_active,"node":node,"resourceLimitsAvailable":false}),
    )
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn plist(label: &str, argv: &[String], path: &str, log: &Path, keep_alive: bool) -> String {
    let argv = argv
        .iter()
        .map(|a| format!("<string>{}</string>", xml(a)))
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array>{argv}</array><key>EnvironmentVariables</key><dict><key>PATH</key><string>{}</string></dict><key>RunAtLoad</key><true/><key>KeepAlive</key>{}<key>AbandonProcessGroup</key><true/><key>ThrottleInterval</key><integer>10</integer><key>Umask</key><integer>63</integer><key>StandardOutPath</key><string>{}</string><key>StandardErrorPath</key><string>{}</string></dict></plist>",
        xml(label),
        xml(path),
        if keep_alive {
            "<true/>"
        } else {
            "<dict><key>SuccessfulExit</key><false/></dict>"
        },
        xml(&log.to_string_lossy()),
        xml(&log.to_string_lossy())
    )
}
async fn install(args: &[String], restart: bool) -> Result<Value> {
    if !args.len().is_multiple_of(2) {
        return Err(Error::Config("host install accepts --herdr PATH"));
    }
    let mut herdr = find("herdr");
    let bun = find("bun");
    for pair in args.chunks_exact(2) {
        match pair[0].as_str() {
            "--herdr" => herdr = Some(pair[1].clone().into()),
            _ => return Err(Error::Config("host install accepts only --herdr PATH")),
        }
    }
    let herdr = fs::canonicalize(herdr.ok_or(Error::Config(
        "install Herdr from https://herdr.dev/tui/docs/install/ first",
    ))?)?;
    let bun = fs::canonicalize(bun.ok_or(Error::Config("bundled Bun runtime is missing"))?)?;
    if !executable(&herdr)
        || !executable(&bun)
        || command(&herdr, &["remote-api-bridge", "--check"]).await? != b"herdr-api-bridge-v1\n"
    {
        return Err(Error::Config("Herdr API bridge is unavailable"));
    }
    command(&bun, &["--version"]).await?;
    let root = default_path(".local/lib/kite/macos")?;
    private(&root)?;
    let binary = std::env::current_exe()?;
    let platform = binary.with_file_name("host-platform");
    if !executable(&platform) {
        return Err(Error::Config("bundled Mac platform component is missing"));
    }
    let digest = kite_agent::fingerprint(&format!(
        "{}:{}:{}",
        hex_digest(&binary).await?,
        hex_digest(&bun).await?,
        hex_digest(&platform).await?
    ));
    let generation = root.join(digest);
    private(&generation)?;
    let source_app = binary
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or(Error::Config("missing host application bundle"))?;
    if source_app.file_name().is_none_or(|n| n != "Kite Host.app")
        || !source_app.join("Contents/Info.plist").is_file()
    {
        return Err(Error::Config("use the bundled Kite Host application"));
    }
    command(
        Path::new("/usr/bin/codesign"),
        &[
            "--verify",
            "--deep",
            "--strict",
            source_app
                .to_str()
                .ok_or(Error::Config("invalid host app path"))?,
        ],
    )
    .await?;
    let staged_app = generation.join("Kite Host.app");
    if !staged_app.exists() {
        command(
            Path::new("/usr/bin/ditto"),
            &[
                source_app
                    .to_str()
                    .ok_or(Error::Config("invalid source path"))?,
                staged_app
                    .to_str()
                    .ok_or(Error::Config("invalid staged path"))?,
            ],
        )
        .await?;
    }
    // Both bundle ID/signing requirement AND the on-disk path remain stable for
    // TCC. Never ask users to authorize a different hash-directory executable.
    let installed_app = root.join("Kite Host.app");
    let next_app = root.join(format!(".Kite-Host-next-{}.app", std::process::id()));
    command(
        Path::new("/usr/bin/ditto"),
        &[
            staged_app
                .to_str()
                .ok_or(Error::Config("invalid staged path"))?,
            next_app
                .to_str()
                .ok_or(Error::Config("invalid next path"))?,
        ],
    )
    .await?;
    command(
        Path::new("/usr/bin/codesign"),
        &[
            "--verify",
            "--deep",
            "--strict",
            next_app
                .to_str()
                .ok_or(Error::Config("invalid next path"))?,
        ],
    )
    .await?;
    let socket = default_path(".local/state/kite/node/host.sock")?;
    private(
        socket
            .parent()
            .ok_or(Error::Config("missing node directory"))?,
    )?;
    let config_path = default_path(".config/kite/node.json")?;
    private(
        config_path
            .parent()
            .ok_or(Error::Config("missing config directory"))?,
    )?;
    let existing = if config_path.exists() {
        let c: Config = serde_json::from_slice(&fs::read(&config_path)?)?;
        if c.launchctl.as_deref() != Some(Path::new("/bin/launchctl")) {
            return Err(Error::Config(
                "existing non-Mac node configuration must be reviewed before migration",
            ));
        }
        Some(c)
    } else {
        None
    };
    if loaded(NODE).await {
        if !restart {
            fs::remove_dir_all(&next_app)?;
            return Err(Error::Config("use host update while the node is running"));
        }
        command(
            Path::new("/bin/launchctl"),
            &["bootout", &format!("{}/{NODE}", domain()?)],
        )
        .await?;
    }
    if installed_app.exists() {
        let previous = root.join(format!(".Kite-Host-previous-{}", std::process::id()));
        fs::rename(&installed_app, &previous)?;
        if let Err(error) = fs::rename(&next_app, &installed_app) {
            let _ = fs::rename(&previous, &installed_app);
            return Err(error.into());
        }
        // The update controller may itself be executing the previous copy.
        // Retain that private backup until a later maintenance pass.
    } else {
        fs::rename(&next_app, &installed_app)?;
    }
    command(Path::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"), &["-f",installed_app.to_str().ok_or(Error::Config("invalid installed app path"))?]).await?;
    let installed_bin = installed_app.join("Contents/MacOS");
    let herdr_external = herdr_running(&herdr).await && !loaded(HERDR).await;
    let mut environment = existing
        .as_ref()
        .map(|c| c.environment.clone())
        .unwrap_or_default();
    environment.insert("PATH".into(), search_path()?);
    let machine = inspect().await?["hostname"]
        .as_str()
        .unwrap_or("Mac")
        .to_owned();
    let config = Config {
        configuration_version: existing
            .as_ref()
            .map_or(0, |config| config.configuration_version),
        sunshine: existing.as_ref().and_then(|config| config.sunshine.clone()),
        node_id: existing
            .as_ref()
            .map(|c| c.node_id.clone())
            .unwrap_or_else(|| {
                format!(
                    "mac-{}",
                    &kite_agent::fingerprint(&format!(
                        "{}:{:?}",
                        machine,
                        std::time::SystemTime::now()
                    ))[..20]
                )
            }),
        machine,
        socket,
        backend: generation.join("host.js"),
        herdr,
        bun: installed_bin.join("bun"),
        systemctl: "/usr/bin/false".into(),
        launchctl: Some("/bin/launchctl".into()),
        agent_memory_limit_bytes: crate::resources::default_agent_memory_limit(),
        source_path: None,
        push_service_account: existing.and_then(|c| c.push_service_account),
        services: vec![Service {
            name: "Herdr".into(),
            unit: HERDR.into(),
            external: herdr_external,
        }],
        environment,
    };
    config.validate()?;
    atomic(&config_path, &serde_json::to_vec_pretty(&config)?, 0o600)?;
    let agents = default_path("Library/LaunchAgents")?;
    fs::create_dir_all(&agents)?;
    let logs = root.join("logs");
    private(&logs)?;
    let bin = default_path(".local/bin")?;
    fs::create_dir_all(&bin)?;
    let link = bin.join("kite-node");
    if link.exists() && !fs::symlink_metadata(&link)?.file_type().is_symlink() {
        return Err(Error::Config(
            "existing kite-node executable must be reviewed before replacement",
        ));
    }
    let pending = bin.join("kite-node.pending");
    symlink(installed_bin.join("kite-node"), &pending)?;
    fs::rename(pending, link)?;
    for (label, argv, keep_alive) in [
        (
            NODE,
            vec![
                installed_bin
                    .join("kite-node")
                    .to_string_lossy()
                    .into_owned(),
                "serve".into(),
                "--config".into(),
                config_path.to_string_lossy().into_owned(),
            ],
            true,
        ),
        (
            HERDR,
            vec![config.herdr.to_string_lossy().into_owned(), "server".into()],
            false,
        ),
    ] {
        if label == HERDR && herdr_external {
            continue;
        }
        let log = logs.join(format!("{label}.log"));
        if !log.exists() {
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&log)?;
        }
        atomic(
            &agents.join(format!("{label}.plist")),
            plist(label, &argv, &search_path()?, &log, keep_alive).as_bytes(),
            0o600,
        )?;
    }
    Ok(json!({"installed":true,"nodeId":config.node_id,"restartRequired":loaded(NODE).await}))
}
async fn hex_digest(path: &Path) -> Result<String> {
    let bytes = command(
        Path::new("/usr/bin/shasum"),
        &[
            "-a",
            "256",
            path.to_str()
                .ok_or(Error::Config("non-UTF8 executable path"))?,
        ],
    )
    .await?;
    Ok(String::from_utf8_lossy(&bytes)
        .split_whitespace()
        .next()
        .ok_or(Error::Protocol("missing binary digest"))?
        .into())
}
fn domain() -> Result<String> {
    Ok(format!("gui/{}", fs::metadata(default_path(".")?)?.uid()))
}
async fn loaded(label: &str) -> bool {
    let Ok(domain) = domain() else { return false };
    command(
        Path::new("/bin/launchctl"),
        &["print", &format!("{domain}/{label}")],
    )
    .await
    .is_ok()
}
async fn enable() -> Result<Value> {
    let config: Config =
        serde_json::from_slice(&fs::read(default_path(".config/kite/node.json")?)?)?;
    config.validate()?;
    if config.launchctl.as_deref() != Some(Path::new("/bin/launchctl")) {
        return Err(Error::Config("host is not configured for launchd"));
    }
    for label in [HERDR, NODE] {
        if label == HERDR && herdr_running(&config.herdr).await {
            continue;
        }
        command(
            Path::new("/bin/launchctl"),
            &["enable", &format!("{}/{label}", domain()?)],
        )
        .await?;
        if loaded(label).await {
            launchd::start(Path::new("/bin/launchctl"), label).await?;
        } else {
            command(
                Path::new("/bin/launchctl"),
                &[
                    "bootstrap",
                    &domain()?,
                    default_path(&format!("Library/LaunchAgents/{label}.plist"))?
                        .to_str()
                        .ok_or(Error::Config("invalid launch agent path"))?,
                ],
            )
            .await?;
        }
    }
    for _ in 0..30 {
        let status = inspect().await?;
        if status["chatAvailable"] == true {
            return Ok(status);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(Error::Unavailable(
        "host did not become ready; inspect private node/Herdr logs".into(),
    ))
}
pub(super) fn run(args: &[String]) -> Result<i32> {
    // All app windows and CLI clients share one bounded installation/lifecycle lease.
    let _lease = if args.first().is_some_and(|s| s != "inspect") {
        let root = default_path(".local/lib/kite/macos")?;
        private(&root)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(root.join("setup.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Config("another Mac host setup is in progress"))?;
        Some(NodeLock(lock))
    } else {
        None
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let result = match args.first().map(String::as_str) {
            Some("inspect") if args.len() == 1 => inspect().await?,
            Some("install") => install(&args[1..],false).await?,
            Some("enable") if args.len() == 1 => enable().await?,
            Some("update") if args.len() == 1 => {
                install(&[],true).await?;
                enable().await?
            }
            Some("disable") if args.len() == 1 => {
                command(
                    Path::new("/bin/launchctl"),
                    &["disable", &format!("{}/{NODE}", domain()?)],
                )
                .await?;
                if loaded(NODE).await {
                    command(
                        Path::new("/bin/launchctl"),
                        &["bootout", &format!("{}/{NODE}", domain()?)],
                    )
                    .await?;
                }
                json!({"disabled":true,"agentsPreserved":true})
            }
            Some("sunshine") if args.len() == 1 => {
                let app = sunshine()
                    .ok_or(Error::Config("install the official Sunshine Mac app first"))?;
                let config = default_path(".config/sunshine/sunshine.conf")?;
                if !config.exists() {
                    private(config.parent().ok_or(Error::Config("missing Sunshine config directory"))?)?;
                    atomic(&config, b"# Kite Mac host: first setup stays on this computer.\norigin_web_ui_allowed = pc\nupnp = disabled\n", 0o600)?;
                }
                command(
                    Path::new("/usr/bin/open"),
                    &[
                        app.to_str().ok_or(Error::Config("invalid Sunshine path"))?,
                        "--args",
                        config
                            .to_str()
                            .ok_or(Error::Config("invalid Sunshine config path"))?,
                    ],
                )
                .await?;
                json!({"requested":true})
            }
            _ => {
                return Err(Error::Config(
                    "host expects inspect, install [--herdr PATH], enable, disable or sunshine",
                ));
            }
        };
        println!("{result}");
        Ok(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_agent_preserves_argv_and_uses_private_logs() {
        let p = plist(
            NODE,
            &["/Users/test/with space/kite-node".into(), "serve".into()],
            "/path&bin",
            Path::new("/private/log"),
            true,
        );
        assert!(p.contains("<string>/Users/test/with space/kite-node</string>"));
        assert!(p.contains("/path&amp;bin"));
        assert!(p.contains("<key>Umask</key><integer>63</integer>"));
        assert!(p.contains("AbandonProcessGroup"));
    }
}
