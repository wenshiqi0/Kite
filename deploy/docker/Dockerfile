# syntax=docker/dockerfile:1
FROM ubuntu:24.04 AS runtime
ARG TARGETARCH
ARG BUN_VERSION=1.3.14
ARG BUN_SHA256=951ee2aee855f08595aeec6225226a298d3fea83a3dcd6465c09cbccdf7e848f
ARG HERDR_VERSION=0.9.5
ARG HERDR_SHA256=14d9e8bc8b4a9e0118d77c32266f76d461f09b40342eb23752960bd89f59929d
ARG SUNSHINE_VERSION=2026.914.233613
ARG SUNSHINE_SHA256=c38e9c705f650f8705f61702717e99bec34044c5028fcb8f23a08fe041292c21
ARG INSTALL_CODEX=0
ARG CODEX_VERSION=0.164.0
ARG EXTRA_APT_PACKAGES=""
ENV container=docker LANG=C.UTF-8 LC_ALL=C.UTF-8
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN test "$TARGETARCH" = amd64 && \
    apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates curl unzip systemd systemd-sysv dbus-user-session dbus-x11 libpam-systemd \
      openssh-server tzdata iproute2 procps psmisc \
      xserver-xorg-core xserver-xorg-video-dummy xserver-xorg-input-libinput \
      x11-xserver-utils x11-utils xauth xdotool xinput xclip \
      xfce4-session xfce4-panel xfwm4 xfdesktop4 xfce4-settings xfce4-terminal thunar libgtk-3-0t64 \
      pulseaudio pulseaudio-utils \
      fonts-noto-cjk fonts-noto-color-emoji fcitx5 fcitx5-pinyin fcitx5-frontend-gtk3 jq $EXTRA_APT_PACKAGES && \
    curl -fSL --retry 3 "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-x64.zip" -o /tmp/bun.zip && \
    echo "$BUN_SHA256  /tmp/bun.zip" | sha256sum -c - && \
    unzip -q /tmp/bun.zip -d /tmp/bun && install /tmp/bun/bun-linux-x64/bun /usr/local/bin/bun && \
    curl -fSL --retry 3 "https://github.com/herdrdev/herdr/releases/download/v${HERDR_VERSION}/herdr-linux-x86_64" -o /usr/local/bin/herdr && \
    echo "$HERDR_SHA256  /usr/local/bin/herdr" | sha256sum -c - && chmod 755 /usr/local/bin/herdr && \
    curl -fSL --retry 3 "https://github.com/LizardByte/Sunshine/releases/download/v${SUNSHINE_VERSION}/sunshine_${SUNSHINE_VERSION}-1%2Bubuntu24.04_amd64.deb" -o /tmp/sunshine.deb && \
    echo "$SUNSHINE_SHA256  /tmp/sunshine.deb" | sha256sum -c - && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends /tmp/sunshine.deb && \
    if [ "$INSTALL_CODEX" = 1 ]; then \
      DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends nodejs && \
      BUN_INSTALL=/opt/kite-tools bun install --global --registry=https://registry.npmjs.org "@openai/codex@${CODEX_VERSION}" && \
      ln -s /opt/kite-tools/bin/codex /usr/local/bin/codex; \
    fi && \
    userdel -r ubuntu && useradd --uid 1000 --create-home --shell /bin/bash kite && \
    install -d -o kite -g kite -m 700 /home/kite/.ssh && \
    passwd -d kite && \
    systemctl mask systemd-udevd.service systemd-udevd-control.socket systemd-udevd-kernel.socket \
      systemd-remount-fs.service getty.target console-getty.service && \
    rm -rf /tmp/bun /tmp/*.deb /tmp/bun.zip /var/lib/apt/lists/* /etc/ssh/ssh_host_* && \
    truncate -s 0 /etc/machine-id

FROM rust:1.97.1-bookworm AS build
COPY --from=runtime /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends cmake pkg-config libopus-dev libx11-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock package.json bun.lock bunfig.toml tsconfig.json ./
COPY crates ./crates
COPY packages ./packages
COPY scripts ./scripts
COPY android/version.properties ./android/version.properties
COPY docs/agent ./docs/agent
COPY skills ./skills
COPY LICENSE ./LICENSE
RUN --mount=type=cache,id=kite-server-bun,target=/root/.bun/install/cache \
    --mount=type=cache,id=kite-server-cargo,target=/usr/local/cargo/registry \
    --mount=type=cache,id=kite-server-target,target=/src/target,sharing=locked \
    bun install --frozen-lockfile && cargo build --release --locked -p kite-node && \
    mkdir -p /opt/kite-host && \
    install -m 755 target/release/kite-node /opt/kite-host/kite-node
ARG SOURCE_COMMIT=unknown
RUN bun build --target=bun --format=esm --packages=bundle packages/linux-host/src/container.ts --outfile /opt/kite-host/container.js && \
    cp LICENSE /opt/kite-host/LICENSE && printf '%s\n' "$SOURCE_COMMIT" > /opt/kite-host/source-commit

FROM runtime AS host
ARG SOURCE_COMMIT=unknown
LABEL org.opencontainers.image.title="Kite Linux server" \
      org.opencontainers.image.source="https://github.com/wenshiqi0/Kite" \
      org.opencontainers.image.licenses="GPL-3.0-only" \
      org.opencontainers.image.revision=$SOURCE_COMMIT
COPY --from=build /opt/kite-host /opt/kite-host
COPY deploy/docker/system/ /etc/systemd/system/
COPY deploy/docker/user/ /etc/systemd/user/
COPY deploy/docker/xorg.conf /etc/X11/kite-xorg.conf
COPY deploy/docker/sshd_config /etc/ssh/sshd_config
COPY deploy/docker/journald.conf /etc/systemd/journald.conf.d/kite.conf
COPY deploy/docker/pulse.pa /etc/pulse/kite.pa
COPY deploy/docker/container.sh /usr/local/sbin/kite-container
COPY deploy/docker/session.sh /usr/local/bin/kite-container-session
COPY deploy/docker/healthcheck.sh /usr/local/bin/kite-container-health
RUN chmod 755 /usr/local/sbin/kite-container /usr/local/bin/kite-container-session /usr/local/bin/kite-container-health && \
    ln -s /opt/kite-host/kite-node /usr/local/bin/kite-node && ln -s kite-node /usr/local/bin/kite && \
    find /etc/systemd/system /etc/systemd/user -type d -exec chmod 755 {} + && \
    find /etc/systemd/system /etc/systemd/user -type f -exec chmod 644 {} + && \
    chmod 644 /etc/pulse/kite.pa && \
    systemctl disable ssh.socket && systemctl mask ssh.socket && \
    systemctl --global mask pulseaudio.service pulseaudio.socket && \
    systemctl enable kite-container-init.service kite-xorg.service ssh.service && \
    systemctl --global enable kite-container-setup.service && \
    mkdir -p /var/lib/systemd/linger && touch /var/lib/systemd/linger/kite && \
    printf '\nexport PATH="$HOME/.local/bin:/opt/kite-tools/bin:/usr/local/bin:$PATH"\nexport XDG_RUNTIME_DIR=/run/user/1000\nexport DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\n' >> /home/kite/.profile
STOPSIGNAL SIGRTMIN+3
HEALTHCHECK --interval=30s --timeout=15s --start-period=180s --retries=3 CMD ["/usr/local/bin/kite-container-health"]
WORKDIR /home/kite
ENTRYPOINT ["/usr/local/sbin/kite-container"]
CMD ["boot"]
