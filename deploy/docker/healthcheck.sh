#!/bin/bash
set -euo pipefail
for unit in ssh.service kite-xorg.service user@1000.service; do
    systemctl is-active --quiet "$unit"
done
exec runuser -u kite -- env XDG_RUNTIME_DIR=/run/user/1000 \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus \
    DISPLAY=:0 XAUTHORITY=/run/kite/Xauthority \
    /bin/bash -euc '
        for unit in kite-node.service herdr.service app-dev.lizardbyte.app.Sunshine.service kite-audio.service kite-desktop-session.service; do
            systemctl --user is-active --quiet "$unit"
        done
        xdpyinfo >/dev/null
        pgrep -u 1000 -x xfce4-session >/dev/null
        pgrep -u 1000 -x xfwm4 >/dev/null
        pgrep -u 1000 -x xfce4-panel >/dev/null
        pgrep -u 1000 -x fcitx5 >/dev/null
        pactl -s unix:/run/user/1000/pulse/native list short sinks | grep -q kite_audio
        /home/kite/.local/bin/kite-node status | jq -e '\'' .health == "ready" '\'' >/dev/null
        systemd-run --user --quiet --wait --pipe --collect -p MemoryMax=64M /usr/bin/true
    '
