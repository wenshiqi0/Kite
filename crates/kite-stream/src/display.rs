//! Read the geometry of the X11 output selected by the running Sunshine process.
//! Never use the SSH login's DISPLAY, phone dimensions, or a list of supported modes.
use crate::StreamError;
use kite_ssh::Session;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesktopSize {
    pub width: u32,
    pub height: u32,
}

impl DesktopSize {
    pub fn validate(self) -> Result<(), StreamError> {
        if !(320..=16384).contains(&self.width) || !(240..=16384).contains(&self.height) {
            return Err(error("invalid captured desktop dimensions"));
        }
        Ok(())
    }
}

pub async fn captured_size(ssh: &Session, port: u16) -> Result<DesktopSize, StreamError> {
    // Only a validated numeric port is interpolated. The script travels over stdin.
    let script = PROBE.replace("{port}", &port.to_string());
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        ssh.exec("/bin/sh -s", script.as_bytes()),
    )
    .await
    .map_err(|_| StreamError::Timeout)??;
    if output.exit_status != Some(0) || output.stdout.len() > 64 * 1024 {
        return Err(error(
            "Sunshine X11 geometry probe failed; requires same-user Sunshine, ss and xrandr",
        ));
    }
    parse_probe(&String::from_utf8_lossy(&output.stdout))
}

fn error(details: &str) -> StreamError {
    StreamError::Display(details.into())
}

fn geometry(value: &str) -> Option<DesktopSize> {
    let (width, rest) = value.split_once('x')?;
    let height = rest.split(['+', '-']).next()?;
    Some(DesktopSize {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
    })
}

fn parse_probe(value: &str) -> Result<DesktopSize, StreamError> {
    let mut lines = value.lines();
    if lines.next() != Some("kite-x11-display-v1") {
        return Err(error("invalid geometry probe response"));
    }
    let output = lines
        .next()
        .and_then(|s| s.strip_prefix("output="))
        .ok_or_else(|| error("missing Sunshine output"))?;
    let mut outputs = Vec::new();
    for line in lines {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 2 && matches!(fields[1], "connected" | "disconnected") {
            let size = fields.iter().skip(2).find_map(|s| geometry(s));
            outputs.push((fields[0], size));
        }
    }
    // XRandR --query lists every output, including disconnected ones, in the same
    // resource order Sunshine's XRRGetScreenResources enumeration uses.
    let selected = if output.is_empty() {
        outputs.first()
    } else if let Ok(index) = output.parse::<usize>() {
        outputs.get(index)
    } else {
        outputs.iter().find(|(name, _)| *name == output)
    };
    let size = selected
        .and_then(|(_, size)| *size)
        .ok_or_else(|| error("selected Sunshine output has no active mode"))?;
    size.validate()?;
    Ok(size)
}

// Read only the few non-secret process attributes needed for capture selection.
// Never evaluate a config file, dump an environment, or change a display mode.
const PROBE: &str = r#"set -eu
export LC_ALL=C
pids=$(ss -H -lntp 'sport = :{port}' | sed -n 's/.*pid=\([0-9][0-9]*\),.*/\1/p' | sort -u)
case "$pids" in ''|*[!0-9]*) exit 1;; esac
test "$(cat "/proc/$pids/comm")" = sunshine
env_value() { tr '\000' '\n' < "/proc/$pids/environ" | sed -n "s/^$1=//p"; }
DISPLAY=$(env_value DISPLAY)
test -n "$DISPLAY"
export DISPLAY
auth=$(env_value XAUTHORITY)
if test -n "$auth"; then XAUTHORITY=$auth; export XAUTHORITY; fi
args=$(tr '\000' '\n' < "/proc/$pids/cmdline")
config=$(printf '%s\n' "$args" | sed -n '2p')
case "$config" in ''|-*|*=*)
  config_home=$(env_value XDG_CONFIG_HOME)
  if test -z "$config_home"; then config_home="$(env_value HOME)/.config"; fi
  config="$config_home/sunshine/sunshine.conf";;
esac
test -r "$config"
setting() {
  { cat "$config"; printf '\n%s\n' "$args"; } | awk -F= -v key="$1" '
  { k=$1; gsub(/^[ \t]+|[ \t]+$/, "", k);
    if(k==key) { v=substr($0,index($0,"=")+1); sub(/[ \t]*#.*/, "", v); gsub(/^[ \t]+|[ \t]+$/, "", v); result=v; } }
  END { print result }'
}
test "$(setting capture)" = x11
printf 'kite-x11-display-v1\noutput=%s\n' "$(setting output_name)"
xrandr --current --query
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUTS: &str = "Screen 0: minimum 320 x 200, current 4480 x 1440, maximum 16384 x 16384\nDP-0 disconnected (normal left inverted right x axis y axis)\nDP-1 connected primary 2560x1440+1920+0 (normal left inverted right x axis y axis)\n   1920x1080 60.0\nHDMI-0 connected 1920x1080+0+0 (normal left inverted right x axis y axis)\n";

    #[test]
    fn selected_output_is_not_virtual_desktop_or_advertised_mode() {
        for output in ["1", "DP-1"] {
            assert_eq!(
                parse_probe(&format!("kite-x11-display-v1\noutput={output}\n{OUTPUTS}")).unwrap(),
                DesktopSize {
                    width: 2560,
                    height: 1440
                }
            );
        }
        assert_eq!(
            parse_probe(&format!("kite-x11-display-v1\noutput=HDMI-0\n{OUTPUTS}")).unwrap(),
            DesktopSize {
                width: 1920,
                height: 1080
            }
        );
    }

    #[test]
    fn portrait_and_negative_monitor_offsets_are_preserved() {
        assert_eq!(
            parse_probe(
                "kite-x11-display-v1\noutput=0\nrdp0 connected 1116x2480-1116+0 (normal)\n"
            )
            .unwrap(),
            DesktopSize {
                width: 1116,
                height: 2480
            }
        );
    }

    #[test]
    fn missing_inactive_and_malformed_outputs_are_errors() {
        for value in [
            format!("kite-x11-display-v1\noutput=0\n{OUTPUTS}"),
            format!("kite-x11-display-v1\noutput=9\n{OUTPUTS}"),
            "bad".into(),
            "kite-x11-display-v1\noutput=0\nrdp0 connected 0x0+0+0\n".into(),
        ] {
            assert!(parse_probe(&value).is_err());
        }
    }
}
