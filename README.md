# Kite

A native mobile companion for [herdr](https://github.com/herdrdev/herdr). Follow your coding agents, open their desktop, and take over when they need a hand.

[Download Android Alpha](https://github.com/wenshiqi0/Kite/releases) · [Set up your host](docs/backend-setup.md)

## What you can do

- Keep a connection to your own machine over SSH.
- Browse herdr agents, read their terminal output, and send prompts.
- Watch the remote desktop through Sunshine, then switch from viewing to mouse and keyboard control.
- Fit the full desktop to your phone while preserving its source resolution, with responsiveness as the priority.

## Get started

1. Install the ARM64 Android APK from [Releases](https://github.com/wenshiqi0/Kite/releases).
2. Prepare a Linux X11 host with SSH, herdr and Sunshine using the [host setup guide](docs/backend-setup.md).
3. Add Kite's public key to your host, verify the host fingerprint, and connect. Open the desktop and pair with Sunshine when prompted.

The Android app is in Alpha and currently uses a Chinese interface. iOS supports the host connection flow; desktop viewing on iOS is still in progress. Audio, structured chat history and picture-in-picture are not available yet.

## Documentation

- [Host setup and troubleshooting](docs/backend-setup.md)
- [Build and development](docs/development.md)
- [Testing](docs/testing.md)
- [Alpha releases and CI](docs/releases.md)
- [Architecture and engineering specification](SPEC.md)

## License

[GPL-3.0-only](LICENSE). Third-party notices are in [NOTICE.txt](NOTICE.txt).
