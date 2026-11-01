//! A new generation accepts the shared listener; the old one drains its sockets.
use super::*;
use std::{
    os::{fd::AsFd, unix::fs::MetadataExt},
    sync::atomic::AtomicBool,
};
use tokio::sync::Notify;

pub(super) struct Handover {
    listener: std::os::unix::net::UnixListener,
    config: PathBuf,
    serial: Mutex<()>,
    pub drain: Notify,
    pub draining: AtomicBool,
}
impl Handover {
    pub fn new(config: &Path, listener: &UnixListener) -> Result<Self> {
        let duplicate = listener.as_fd().try_clone_to_owned()?;
        Ok(Self {
            listener: duplicate.into(),
            config: fs::canonicalize(config)?,
            serial: Mutex::new(()),
            drain: Notify::new(),
            draining: AtomicBool::new(false),
        })
    }
}

pub(super) fn inherited_listener(expected: &Path) -> Result<UnixListener> {
    let fd = std::io::stdin().as_fd().try_clone_to_owned()?;
    let listener = std::os::unix::net::UnixListener::from(fd);
    if listener.local_addr()?.as_pathname() != Some(expected) {
        return Err(Error::Config(
            "inherited listener does not match configuration",
        ));
    }
    listener.set_nonblocking(true)?;
    Ok(UnixListener::from_std(listener)?)
}

pub(super) async fn notify(config: &Config, pid: u32) -> Result<()> {
    if std::env::var_os("NOTIFY_SOCKET").is_none() {
        return Err(Error::Config(
            "live upgrade requires a notifying service manager",
        ));
    }
    let mut command = Command::new(config.systemctl.with_file_name("systemd-notify"));
    command.args([format!("--pid={pid}"), "--ready".into()]);
    // Keep systemd-notify's default barrier: MAINPID must be acknowledged before
    // an empty old generation exits. NotifyAccess=all allows this helper.
    bounded_output_with_timeout(command, Duration::from_secs(8)).await?;
    Ok(())
}

pub(super) async fn activate_replacement(node: &Node) -> Result<()> {
    if node.notification_problem.is_some() || node.push_problem.is_some() {
        return Err(Error::Protocol(
            "replacement persistent state is unavailable",
        ));
    }
    let fd = std::io::stdout().as_fd().try_clone_to_owned()?;
    let channel = std::os::unix::net::UnixStream::from(fd);
    channel.set_nonblocking(true)?;
    let mut channel = BufReader::new(UnixStream::from_std(channel)?);
    json_line(channel.get_mut(), &json!({"ready":true,"pid":std::process::id(),"api":API,"backendProtocol":5,"nodeId":node.config.node_id})).await?;
    let command = tokio::time::timeout(Duration::from_secs(12), read_json(&mut channel))
        .await
        .map_err(|_| Error::Protocol("upgrade activation timed out"))??;
    if command["activate"] != true {
        return Err(Error::Protocol("upgrade activation rejected"));
    }
    json_line(channel.get_mut(), &json!({"active":true})).await?;
    let committed = tokio::time::timeout(Duration::from_secs(12), read_json(&mut channel))
        .await
        .map_err(|_| Error::Protocol("upgrade commit timed out"))??;
    if committed["committed"] != true {
        return Err(Error::Protocol("upgrade commit rejected"));
    }
    Ok(())
}

pub(super) async fn prepare_replacement() -> Result<()> {
    let fd = std::io::stdout().as_fd().try_clone_to_owned()?;
    let channel = std::os::unix::net::UnixStream::from(fd);
    channel.set_nonblocking(true)?;
    let mut channel = BufReader::new(UnixStream::from_std(channel)?);
    json_line(
        channel.get_mut(),
        &json!({"spawned":true,"pid":std::process::id()}),
    )
    .await?;
    let command = tokio::time::timeout(Duration::from_secs(10), read_json(&mut channel))
        .await
        .map_err(|_| Error::Protocol("generation prepare timed out"))??;
    if command["prepare"] != true {
        return Err(Error::Protocol("generation prepare rejected"));
    }
    Ok(())
}

struct Generation {
    pid: u32,
    ticks: String,
    inode: u64,
    device: u64,
}
impl Generation {
    fn inspect(pid: u32, binary: &Path) -> Result<Self> {
        if pid <= 1 || pid == std::process::id() {
            return Err(Error::Protocol("invalid generation pid"));
        }
        let process = PathBuf::from(format!("/proc/{pid}"));
        let executable = fs::metadata(process.join("exe"))?;
        let expected = fs::metadata(binary)?;
        if executable.ino() != expected.ino()
            || executable.dev() != expected.dev()
            || fs::read(process.join("cgroup"))? != fs::read("/proc/self/cgroup")?
        {
            return Err(Error::Protocol("replacement process identity mismatch"));
        }
        let stat = fs::read_to_string(process.join("stat"))?;
        let ticks = stat
            .rsplit_once(") ")
            .and_then(|(_, fields)| fields.split_whitespace().nth(19))
            .ok_or(Error::Protocol("generation process state missing"))?
            .to_owned();
        Ok(Self {
            pid,
            ticks,
            inode: executable.ino(),
            device: executable.dev(),
        })
    }
    async fn terminate(&self) {
        let process = PathBuf::from(format!("/proc/{}", self.pid));
        let same = (|| -> Option<bool> {
            let executable = fs::metadata(process.join("exe")).ok()?;
            let stat = fs::read_to_string(process.join("stat")).ok()?;
            let ticks = stat.rsplit_once(") ")?.1.split_whitespace().nth(19)?;
            Some(
                executable.ino() == self.inode
                    && executable.dev() == self.device
                    && ticks == self.ticks,
            )
        })();
        if same == Some(true) {
            let mut command = Command::new("/bin/kill");
            command.args(["-KILL", "--", &self.pid.to_string()]);
            let _ = bounded_output_with_timeout(command, Duration::from_secs(2)).await;
        }
    }
}

pub(super) async fn handover(node: &Node, request: &NodeRequest) -> Result<Value> {
    let upgrade = node.upgrade.as_ref().ok_or(Error::Config(
        "node is not socket activated; explicit maintenance migration required",
    ))?;
    let _serial = upgrade
        .serial
        .try_lock()
        .map_err(|_| Error::Protocol("upgrade already in progress"))?;
    if node.draining() {
        return Err(Error::Protocol("generation is already draining"));
    }
    let binary = Path::new(request.params["binary"].as_str().unwrap_or_default());
    if !binary.is_absolute() {
        return Err(Error::Config("upgrade binary must be absolute"));
    }
    let binary = fs::canonicalize(binary)?;
    let mut probe = Command::new(&binary);
    probe.arg("version");
    let metadata: Value =
        serde_json::from_slice(&bounded_output_with_timeout(probe, Duration::from_secs(3)).await?)?;
    if metadata["api"] != API
        || metadata["backendProtocol"] != 5
        || metadata["liveUpgrade"] != 1
        || metadata["generationLaunch"] != 1
    {
        return Err(Error::Protocol("incompatible upgrade candidate"));
    }
    // Current observation/notification delivery is allowed to finish; no two
    // generations may update snapshots of the same notification/push ledger.
    let _observers = tokio::time::timeout(Duration::from_secs(20), node.observation_gate.write())
        .await
        .map_err(|_| Error::Protocol("observers busy; upgrade not started"))?;
    let (parent, child_channel) = std::os::unix::net::UnixStream::pair()?;
    parent.set_nonblocking(true)?;
    let mut channel = BufReader::new(UnixStream::from_std(parent)?);
    let listener = upgrade.listener.as_fd().try_clone_to_owned()?;
    let mut child = Command::new(&binary)
        .args(["spawn-generation", "--config"])
        .arg(&upgrade.config)
        .stdin(Stdio::from(listener))
        .stdout(Stdio::from(std::os::fd::OwnedFd::from(child_channel)))
        .stderr(Stdio::inherit())
        .kill_on_drop(false)
        .spawn()?;
    let mut generation = None;
    let mut notified = false;
    let result: Result<()> = async {
        let launched = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .map_err(|_| Error::Protocol("generation launcher timed out"))??;
        if !launched.success() {
            return Err(Error::Protocol("generation launcher failed"));
        }
        let spawned = tokio::time::timeout(Duration::from_secs(5), read_json(&mut channel))
            .await
            .map_err(|_| Error::Protocol("generation identity timed out"))??;
        let pid = spawned["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or(Error::Protocol("generation pid missing"))?;
        if spawned["spawned"] != true {
            return Err(Error::Protocol("invalid generation bootstrap"));
        }
        generation = Some(Generation::inspect(pid, &binary)?);
        json_line(channel.get_mut(), &json!({"prepare":true})).await?;
        let ready = tokio::time::timeout(Duration::from_secs(10), read_json(&mut channel))
            .await
            .map_err(|_| Error::Protocol("replacement readiness timed out"))??;
        if ready["ready"] != true
            || ready["pid"] != pid
            || ready["api"] != API
            || ready["backendProtocol"] != 5
            || ready["nodeId"] != node.config.node_id
        {
            return Err(Error::Protocol("invalid replacement readiness"));
        }
        json_line(channel.get_mut(), &json!({"activate":true})).await?;
        let active = tokio::time::timeout(Duration::from_secs(4), read_json(&mut channel))
            .await
            .map_err(|_| Error::Protocol("replacement commit timed out"))??;
        if active["active"] != true {
            return Err(Error::Protocol("replacement did not activate"));
        }
        // The candidate still waits for commit and cannot serve requests or
        // scan/write shared ledgers while MAINPID ownership is being confirmed.
        notified = true;
        notify(&node.config, pid).await?;
        json_line(channel.get_mut(), &json!({"committed":true})).await?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        if notified {
            // If the manager cannot acknowledge rollback, leave the candidate
            // supervised rather than killing a process still marked MAINPID.
            if notify(&node.config, std::process::id()).await.is_err() {
                return Err(Error::Protocol(
                    "upgrade rollback was not acknowledged; inspect service manager",
                ));
            }
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
        if let Some(generation) = &generation {
            generation.terminate().await;
        }
        return Err(error);
    }
    // Ownership has moved to systemd. It supervises this child after the old
    // generation drains/exits, so dropping our Child must never terminate it.
    upgrade.draining.store(true, Ordering::Release);
    upgrade.drain.notify_one();
    let pid = generation
        .ok_or(Error::Protocol("committed generation identity missing"))?
        .pid;
    Ok(
        json!({"upgraded":true,"pid":pid,"version":metadata["version"],"previousPid":std::process::id()}),
    )
}
