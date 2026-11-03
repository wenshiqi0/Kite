# Docker server acceptance — 2026-11-03

The Docker image is a **server-only** deployment: Kite Node, Herdr 0.9.5, Sunshine 2026.914.233613 and Bun 1.3.14. Xorg/Xfce supplies the server desktop captured by Sunshine. The image has no `kite-desktop` executable, Kite panel/autostart entry or Kite client. Default language is English (`C.UTF-8`), with Noto Chinese fonts and Fcitx 5 Pinyin. The captured fresh 1920×1080 Xfce desktop was visually inspected.

Local validation used a uniquely named, disposable Docker project, private PID/cgroup namespaces, an owned public key and fresh named volumes. Linux `uinput`/`uhid` are host prerequisites. The local test host needed these modules loaded; the deploy command now checks that prerequisite and Xfce before creating the persistent deployment. No personal authorization, provider login, model prompt or existing project was copied into the container.

- `bun run check`: strict typecheck, Biome, dependency boundaries and **65 tests passed**.
- Rustfmt and Clippy for `docker_desktop_probe`, with warnings denied: passed.
- Real SSH on the supplied custom port, Herdr API and Node health: passed.
- Server-only binary/autostart checks, actual 1920×1080 PNG, Xfce/IME process health, Chinese fonts and English locale: passed.
- Virtual PulseAudio sink and a real delegated cgroup `memory.max = 67108864`: passed.
- Native probe: paired with the owned Sunshine instance, received **170 encoded video frames** and Opus data, and delivered a mouse click and F8 key to the owned GTK fixture. Requested format was 1920×1080 at 60 fps; this is encoded transport/input validation, not a decoded or displayed frame-rate measurement.
- After stopping and recreating the container with the same named volumes and without importing a key again, SSH fingerprint, Kite node identity and the owned workspace marker were unchanged.

The probe removed its containers, volumes and temporary private key. CI runs the same guarded localhost probe. Physical-phone decoding, speaker output, GPU encoding and WAN latency were not tested. Server deployment and operating instructions are in [Docker host deployment](../../../docker-host.md).
