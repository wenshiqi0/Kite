#!/bin/bash
set -euo pipefail
fail() { echo "Kite container: $*" >&2; exit 1; }
authorize() {
    install -d -o kite -g kite -m 700 /home/kite/.ssh
    local keys=/home/kite/.ssh/authorized_keys imported=/var/lib/kite-container/authorized_keys.import
    if [[ -f $imported ]]; then
        touch "$keys"
        awk 'NF && !seen[$0]++' "$keys" "$imported" > "$keys.new"
        mv "$keys.new" "$keys"
        rm "$imported"
    fi
    [[ -s $keys ]] || fail "deploy with --authorized-keys before starting"
    chown kite:kite /home/kite "$keys"
    chmod 600 "$keys"
}

case "${1:-boot}" in
boot)
    [[ $(id -u) = 0 ]] || fail "boot requires container root"
    [[ -f /sys/fs/cgroup/cgroup.controllers && -w /sys/fs/cgroup ]] || fail "a writable, private cgroup v2 namespace is required"
    [[ -d /run/udev/data && -d /dev/input ]] || fail "mount the server's /run/udev and /dev/input read-only"
    install -d -m 755 /run/kite
    : > /run/kite/container.env
    chmod 600 /run/kite/container.env
    for name in KITE_SSH_PORT KITE_SUNSHINE_PORT KITE_DESKTOP_MODE KITE_ENCODER KITE_RENDER_DEVICE TZ; do
        if [[ -v $name ]]; then
            value=${!name}
            [[ $value =~ ^[A-Za-z0-9_./:+-]+$ ]] || fail "invalid $name"
            printf '%s=%s\n' "$name" "$value" >> /run/kite/container.env
        fi
    done
    if [[ -s /var/lib/kite-container/machine-id ]]; then
        cp /var/lib/kite-container/machine-id /etc/machine-id
    fi
    exec /sbin/init
    ;;
initialize)
    umask 077
    ssh_port=${KITE_SSH_PORT:-2222}
    sunshine_port=${KITE_SUNSHINE_PORT:-48089}
    [[ $ssh_port =~ ^[0-9]+$ && $ssh_port -ge 1024 && $ssh_port -le 65535 ]] || fail "invalid SSH port"
    [[ $sunshine_port =~ ^[0-9]+$ && $sunshine_port -ge 1029 && $sunshine_port -le 65514 ]] || fail "invalid Sunshine base port"
    for port in "$((sunshine_port-5))" "$sunshine_port" "$((sunshine_port+1))" "$((sunshine_port+21))"; do
        [[ $port != "$ssh_port" ]] || fail "SSH and Sunshine ports overlap"
    done
    mode=${KITE_DESKTOP_MODE:-1920x1080}
    [[ $mode =~ ^(1280x720|1920x1080|2560x1440)$ ]] || fail "unsupported desktop mode"
    encoder=${KITE_ENCODER:-software}
    [[ $encoder = software || $encoder = vaapi || $encoder = nvenc ]] || fail "unsupported encoder"
    render=${KITE_RENDER_DEVICE:-/dev/dri/renderD128}
    [[ $render =~ ^/dev/dri/renderD[0-9]+$ ]] || fail "invalid VAAPI render device"
    install -d -m 755 /run/kite /run/sshd
    install -d -m 700 /var/lib/kite-container/ssh
    [[ -s /var/lib/kite-container/machine-id ]] || cp /etc/machine-id /var/lib/kite-container/machine-id
    for kind in ed25519 rsa; do
        key=/var/lib/kite-container/ssh/ssh_host_${kind}_key
        [[ -s $key ]] || ssh-keygen -q -t "$kind" -N '' -f "$key"
    done
    printf 'Port %s\n' "$ssh_port" > /run/kite/sshd.conf
    authorize
    # Create private device nodes. Never chmod/chown the server's device nodes.
    [[ -e /sys/class/misc/uinput/dev && -e /sys/class/misc/uhid/dev ]] || fail "load the server's uinput and uhid kernel modules"
    for spec in 'uinput 223' 'uhid 239'; do
        read -r device minor <<< "$spec"
        rm -f "/dev/$device"
        mknod -m 660 "/dev/$device" c 10 "$minor"
        chown root:kite "/dev/$device"
    done
    if [[ $encoder = vaapi ]]; then
        [[ -c $render ]] || fail "VAAPI render device is unavailable"
        group_id=$(stat -c %g "$render")
        group_name=$(getent group "$group_id" | cut -d: -f1)
        if [[ -z $group_name ]]; then group_name=kite-render; groupadd -g "$group_id" "$group_name"; fi
        usermod -a -G "$group_name" kite
    fi
    touch /run/kite/Xauthority
    xauth -f /run/kite/Xauthority add :0 . "$(mcookie)"
    chown kite:kite /run/kite/Xauthority
    chmod 600 /run/kite/Xauthority
    cat > /run/kite/session.env <<EOF
DISPLAY=:0
XAUTHORITY=/run/kite/Xauthority
XDG_RUNTIME_DIR=/run/user/1000
DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
PULSE_SERVER=unix:/run/user/1000/pulse/native
PATH=/home/kite/.local/bin:/opt/kite-tools/bin:/usr/local/bin:/usr/bin:/bin
LANG=C.UTF-8
LC_ALL=C.UTF-8
TZ=${TZ:-UTC}
GTK_IM_MODULE=fcitx
QT_IM_MODULE=fcitx
XMODIFIERS=@im=fcitx
KITE_DESKTOP_MODE=$mode
EOF
    chmod 644 /run/kite/session.env
    cp /run/kite/session.env /etc/environment
    chmod 644 /etc/environment
    for unit in kite-container-setup herdr app-dev.lizardbyte.app.Sunshine kite-audio kite-desktop-session kite-node; do
        install -d -m 755 "/etc/systemd/user/${unit}.service.d"
        printf '[Service]\nEnvironmentFile=/run/kite/session.env\n' > "/etc/systemd/user/${unit}.service.d/container.conf"
        chmod 644 "/etc/systemd/user/${unit}.service.d/container.conf"
    done
    # Configuration belongs to the persistent user. Seed it only on first boot.
    install -d -o kite -g kite -m 700 /home/kite/.config /home/kite/.config/sunshine
    if [[ ! -e /home/kite/.config/sunshine/sunshine.conf ]]; then
        cat > /home/kite/.config/sunshine/sunshine.conf <<EOF
sunshine_name = Kite Docker
capture = x11
encoder = $encoder
adapter_name = $render
output_name = 0
port = $sunshine_port
address_family = ipv4
bind_address = 0.0.0.0
upnp = disabled
origin_web_ui_allowed = pc
system_tray = disabled
dd_configuration_option = disabled
audio_sink = kite_audio
EOF
        chown kite:kite /home/kite/.config/sunshine/sunshine.conf
    fi
    # A changed base port must not silently disagree with an existing pairing/config.
    configured_port=$(sed -n 's/^port *= *//p' /home/kite/.config/sunshine/sunshine.conf)
    [[ $configured_port = "$sunshine_port" ]] || fail "KITE_SUNSHINE_PORT differs from the persistent Sunshine configuration"
    ;;
authorize) authorize ;;
*) fail "expected boot, initialize or authorize" ;;
esac
