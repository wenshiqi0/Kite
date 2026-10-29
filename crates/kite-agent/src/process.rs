//! Explicit Linux process-group pause. Identity is checked before every signal.
use kite_herdr::api::{Agent, Api};
use kite_ssh::Session;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Api(#[from] kite_herdr::api::ApiError),
    #[error(transparent)]
    Ssh(#[from] kite_ssh::SshError),
    #[error("agent process identity could not be verified")]
    Identity,
    #[error("agent and terminal wrapper have different pause states; resume to reconcile")]
    Partial,
    #[error("terminal foreground group changed; restore the job in its host terminal")]
    Foreground,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellToken {
    pub group: u32,
    pub started: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub group: u32,
    pub started: u64,
    pub shell: Option<ShellToken>,
}

pub async fn prepare(api: &Api, ssh: &Session, agent: &Agent) -> Result<Token, Error> {
    let result = api.process_info(&agent.pane_id).await?;
    let info = &result["process_info"];
    let group = info["foreground_process_group_id"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(Error::Identity)?;
    if group <= 1 {
        return Err(Error::Identity);
    }
    let provider = agent
        .agent_session
        .as_ref()
        .map(|s| s.agent.as_str())
        .or(agent.agent.as_deref())
        .ok_or(Error::Identity)?;
    if !matches!(provider, "claude" | "codex") {
        return Err(Error::Identity);
    }
    let matched = info["foreground_processes"]
        .as_array()
        .is_some_and(|processes| {
            processes.iter().any(|p| {
                p["name"]
                    .as_str()
                    .is_some_and(|n| n.to_lowercase().contains(provider))
                    || p["argv"].as_array().is_some_and(|args| {
                        args.iter().any(|a| {
                            a.as_str()
                                .is_some_and(|s| s.to_lowercase().contains(provider))
                        })
                    })
            })
        });
    if !matched {
        return Err(Error::Identity);
    }
    let (started, stopped) = inspect(ssh, group, None, "inspect").await?;
    if stopped {
        return Err(Error::Partial);
    }
    let shell = if let Some(pid) = info["shell_pid"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n != group)
    {
        let (started, stopped) = inspect(ssh, pid, None, "inspect").await?;
        if stopped {
            return Err(Error::Partial);
        }
        Some(ShellToken {
            group: pid,
            started,
        })
    } else {
        None
    };
    Ok(Token {
        group,
        started,
        shell,
    })
}

pub async fn paused(ssh: &Session, token: &Token) -> Result<bool, Error> {
    let child = inspect(ssh, token.group, Some(token.started), "inspect").await;
    let parent = if let Some(shell) = &token.shell {
        Some(inspect(ssh, shell.group, Some(shell.started), "inspect").await)
    } else {
        None
    };
    match (child, parent) {
        (Ok((_, a)), None) => Ok(a),
        (Ok((_, a)), Some(Ok((_, b)))) if a == b => Ok(a),
        (Ok(_), Some(Ok(_)))
        | (Err(Error::Identity), Some(Ok((_, true))))
        | (Ok((_, true)), Some(Err(Error::Identity))) => Err(Error::Partial),
        (Err(e), Some(Err(parent))) => {
            if matches!(parent, Error::Identity) {
                Err(e)
            } else {
                Err(parent)
            }
        }
        (Err(e), _) | (_, Some(Err(e))) => Err(e),
    }
}

pub async fn signal(ssh: &Session, token: &Token, pause: bool) -> Result<(), Error> {
    if pause && let Some(shell) = &token.shell {
        let (_, stopped) = inspect(ssh, shell.group, Some(shell.started), "stop-shell").await?;
        if !stopped {
            return Err(Error::Partial);
        }
    }
    let child = inspect(
        ssh,
        token.group,
        Some(token.started),
        if pause { "stop" } else { "continue" },
    )
    .await;
    if pause {
        if let Err(error) = child {
            if inspect(ssh, token.group, Some(token.started), "continue")
                .await
                .is_ok()
                && let Some(shell) = &token.shell
            {
                let _ = inspect(ssh, shell.group, Some(shell.started), "continue-shell").await;
            }
            return Err(error);
        }
    } else {
        match child {
            Ok((_, false)) | Err(Error::Identity) => {}
            Ok((_, true)) => return Err(Error::Partial),
            Err(e) => return Err(e),
        }
        if let Some(shell) = &token.shell {
            match inspect(ssh, shell.group, Some(shell.started), "continue-shell").await {
                Ok((_, false)) | Err(Error::Identity) => {}
                Ok((_, true)) => return Err(Error::Partial),
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }
    if paused(ssh, token).await? {
        Ok(())
    } else {
        Err(Error::Partial)
    }
}

async fn inspect(
    ssh: &Session,
    group: u32,
    expected: Option<u64>,
    operation: &str,
) -> Result<(u64, bool), Error> {
    if group <= 1 || group > i32::MAX as u32 {
        return Err(Error::Identity);
    }
    let script = format!(
        r#"set -eu
pid={group}
expected={expected}
operation={operation}
test "$(stat -c %u /proc/$pid)" = "$(id -u)"
line=$(cat /proc/$pid/stat)
rest=${{line##*) }}
set -- $rest
test "$3" = "$pid"
started=${{20}}
before_state=$1
if test "$expected" != 0; then test "$started" = "$expected"; fi
if test "$operation" = stop || test "$operation" = continue; then
  if test "$6" != "$pid"; then exit 5; fi
fi
own=$(cat /proc/$$/stat)
own=${{own##*) }}
set -- $own
test "$3" != "$pid"
case "$operation" in
  stop|stop-shell) if test "$before_state" != T && test "$before_state" != t; then /bin/kill -STOP -- -"$pid"; fi;;
  continue|continue-shell) if test "$before_state" = T || test "$before_state" = t; then /bin/kill -CONT -- -"$pid"; fi;;
  inspect) ;;
  *) exit 2;;
esac
if test "$operation" != inspect; then sleep 0.05; fi
line=$(cat /proc/$pid/stat)
rest=${{line##*) }}
set -- $rest
test "${{20}}" = "$started"
printf '%s %s\n' "$started" "$1"
"#,
        expected = expected.unwrap_or(0)
    );
    let output = ssh.exec("/bin/sh -s", script.as_bytes()).await?;
    if output.exit_status == Some(5) {
        return Err(Error::Foreground);
    }
    if output.exit_status != Some(0) || output.stdout.len() > 128 {
        return Err(Error::Identity);
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| Error::Identity)?;
    let parts = text.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(Error::Identity);
    }
    Ok((
        parts[0].parse().map_err(|_| Error::Identity)?,
        matches!(parts[1], "T" | "t"),
    ))
}
