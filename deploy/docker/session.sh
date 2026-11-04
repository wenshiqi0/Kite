#!/bin/bash
set -euo pipefail
export HOME=/home/kite
umask 077
case "${1:-}" in
setup)
    # Fail before enabling host services when an incomplete/derived image lacks Xfce.
    for executable in xfce4-session xfce4-panel xfwm4 xfdesktop fcitx5; do
        command -v "$executable" >/dev/null || { echo "missing desktop component: $executable" >&2; exit 1; }
    done
    fc-list :lang=zh family | grep -q . || { echo 'Chinese fonts are unavailable' >&2; exit 1; }
    if [[ ! -e $HOME/.config/fcitx5/profile ]]; then
        mkdir -p "$HOME/.config/fcitx5"
        cat > "$HOME/.config/fcitx5/profile" <<'EOF'
[Groups/0]
Name=Default
Default Layout=us
DefaultIM=pinyin

[Groups/0/Items/0]
Name=keyboard-us
Layout=

[Groups/0/Items/1]
Name=pinyin
Layout=

[GroupOrder]
0=Default
EOF
    fi
    for attempt in {1..100}; do
        if xdpyinfo >/dev/null 2>&1; then break; fi
        sleep 0.1
    done
    xdpyinfo >/dev/null
    xrandr --output DUMMY0 --mode "${KITE_DESKTOP_MODE:-1920x1080}"
    bun /opt/kite-host/container.js
    systemctl --user daemon-reload
    systemctl --user enable kite-node.socket kite-node.service herdr.service \
        app-dev.lizardbyte.app.Sunshine.service kite-audio.service kite-desktop-session.service
    systemctl --user start --no-block kite-node.socket kite-node.service herdr.service \
        app-dev.lizardbyte.app.Sunshine.service kite-audio.service kite-desktop-session.service
    ;;
audio)
    exec pulseaudio --daemonize=no --exit-idle-time=-1 --disallow-exit -n --file=/etc/pulse/kite.pa
    ;;
desktop)
    xset s off
    xset -dpms || true
    fcitx5 -d
    xfconf-query -c xfwm4 -p /general/use_compositing -n -t bool -s false
    exec xfce4-session
    ;;
*) echo 'expected setup, audio or desktop' >&2; exit 1 ;;
esac
