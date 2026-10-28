//! herdr integration over SSH.
//!
//! herdr ships its own SSH bridges (`herdr remote-api-bridge` and
//! `herdr remote-client-bridge`), so the host never opens a port. The first
//! step is finding a herdr binary on the host that speaks the API bridge.

use std::time::Duration;

use kite_ssh::{Session, SshError};

/// The line `herdr remote-api-bridge --check` prints when the bridge is supported.
pub const API_BRIDGE_CAPABILITY: &str = "herdr-api-bridge-v1";

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// The herdr binary that answered the bridge check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeCheck {
    pub herdr_path: String,
    pub capability: String,
    /// First line of `herdr --version`, if it printed one.
    pub version: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error("herdr was not found on the host")]
    NotFound,
    #[error("herdr at {path} does not support the API bridge (answered `{capability}`)")]
    Unsupported { path: String, capability: String },
    #[error("timed out while probing for herdr")]
    Timeout,
    #[error("unexpected probe result: {0}")]
    Probe(String),
}

/// Finds herdr on the host and runs `herdr remote-api-bridge --check`.
///
/// With `herdr_path` set, only that binary is tried; otherwise the login
/// shell's `PATH` and the usual install locations are searched.
pub async fn check_api_bridge(
    session: &Session,
    herdr_path: Option<&str>,
) -> Result<BridgeCheck, CheckError> {
    let script = probe_script(herdr_path);
    // The script goes through stdin so the user's login shell, which may not
    // be POSIX, never has to parse it.
    let output = tokio::time::timeout(PROBE_TIMEOUT, session.exec("/bin/sh -s", script.as_bytes()))
        .await
        .map_err(|_| CheckError::Timeout)??;
    parse_probe(&String::from_utf8_lossy(&output.stdout), output.exit_status)
}

fn probe_script(herdr_path: Option<&str>) -> String {
    let candidates = match herdr_path {
        Some(path) => format!("emit {}", shell_quote(path)),
        None => DEFAULT_CANDIDATES.to_string(),
    };
    PROBE_TEMPLATE.replace("{candidates}", &candidates)
}

const DEFAULT_CANDIDATES: &str = r#"command -v herdr 2>/dev/null
"${SHELL:-/bin/sh}" -lc 'command -v herdr' </dev/null 2>/dev/null
emit "${HOME:-}/.local/bin/herdr"
emit "${HOME:-}/.cargo/bin/herdr"
emit /opt/homebrew/bin/herdr
emit /usr/local/bin/herdr
emit /home/linuxbrew/.linuxbrew/bin/herdr
emit "${HOME:-}/.nix-profile/bin/herdr"
emit "/etc/profiles/per-user/${USER:-}/bin/herdr"
emit /nix/var/nix/profiles/default/bin/herdr
emit /run/current-system/sw/bin/herdr"#;

/// Prints `path=`, `capability=` and `version=` lines for the first candidate
/// whose check answers `herdr-api-bridge-v1`. Exits 2 with the first runnable
/// candidate's answer when none does, and 127 when there is no candidate.
const PROBE_TEMPLATE: &str = r#"set -f
emit() {
    if [ -n "$1" ] && [ -x "$1" ]; then printf '%s\n' "$1"; fi
}
candidates=$(
{candidates}
)
IFS='
'
fallback=
fallback_capability=
for candidate in $candidates; do
    case "$candidate" in
        */mise/shims/herdr) continue ;;
        /*) ;;
        *) continue ;;
    esac
    [ -x "$candidate" ] || continue
    capability=$("$candidate" remote-api-bridge --check </dev/null 2>/dev/null | head -n 1)
    if [ "$capability" = herdr-api-bridge-v1 ]; then
        printf 'path=%s\n' "$candidate"
        printf 'capability=%s\n' "$capability"
        printf 'version=%s\n' "$("$candidate" --version </dev/null 2>/dev/null | head -n 1)"
        exit 0
    fi
    if [ -z "$fallback" ]; then
        fallback=$candidate
        fallback_capability=$capability
    fi
done
if [ -n "$fallback" ]; then
    printf 'path=%s\n' "$fallback"
    printf 'capability=%s\n' "$fallback_capability"
    exit 2
fi
exit 127
"#;

fn parse_probe(stdout: &str, exit_status: Option<u32>) -> Result<BridgeCheck, CheckError> {
    let field = |name: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(str::to_owned)
    };
    match exit_status {
        Some(0) => {
            let herdr_path = field("path").ok_or_else(|| CheckError::Probe(stdout.to_owned()))?;
            let capability = field("capability").unwrap_or_default();
            if capability != API_BRIDGE_CAPABILITY {
                return Err(CheckError::Probe(stdout.to_owned()));
            }
            Ok(BridgeCheck {
                herdr_path,
                capability,
                version: field("version").filter(|version| !version.is_empty()),
            })
        }
        Some(2) => Err(CheckError::Unsupported {
            path: field("path").unwrap_or_default(),
            capability: field("capability").unwrap_or_default(),
        }),
        Some(127) => Err(CheckError::NotFound),
        other => Err(CheckError::Probe(format!(
            "exit status {other:?}, output: {stdout}"
        ))),
    }
}

/// Quotes `value` as a single POSIX shell word.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_supported_herdr() {
        let check = parse_probe(
            "path=/home/a/.local/bin/herdr\ncapability=herdr-api-bridge-v1\nversion=herdr 0.9.5\n",
            Some(0),
        )
        .unwrap();
        assert_eq!(
            check,
            BridgeCheck {
                herdr_path: "/home/a/.local/bin/herdr".into(),
                capability: API_BRIDGE_CAPABILITY.into(),
                version: Some("herdr 0.9.5".into()),
            }
        );
    }

    #[test]
    fn reports_an_old_herdr_as_unsupported() {
        let error = parse_probe("path=/usr/local/bin/herdr\ncapability=\n", Some(2)).unwrap_err();
        assert!(matches!(
            error,
            CheckError::Unsupported { path, capability } if path == "/usr/local/bin/herdr" && capability.is_empty()
        ));
    }

    #[test]
    fn reports_a_missing_herdr() {
        assert!(matches!(
            parse_probe("", Some(127)),
            Err(CheckError::NotFound)
        ));
    }

    #[test]
    fn quotes_explicit_paths() {
        assert_eq!(shell_quote("/opt/it's/herdr"), r"'/opt/it'\''s/herdr'");
        assert!(probe_script(Some("/x/herdr")).contains("emit '/x/herdr'"));
    }
}
