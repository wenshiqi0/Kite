# Mac client checks

`macos/scripts/test.sh` runs the offline checks and skips live integration. H.264 and Opus fixtures are synthetic data generated with FFmpeg; no recorded user media is included.

Live integration is explicit and uses a different bundle identifier from the installed client. Supply an absolute JSON path through the `KITE_MAC_LIVE_CONFIG` Xcode build setting:

```sh
macos/scripts/test.sh KITE_MAC_LIVE_CONFIG="$PWD/macos/build/live-qa.json"
```

The configuration contains `host` (a resolvable `.local` hostname), `username`, `fingerprint` (verified independently over an already trusted connection), `project` (the remote disposable path ending in `/.cache/kite-macos-client-20261102/project`), `output` (a local private evidence directory) and `secrets` (a local private test-secret directory).

Generate a disposable OpenSSH ed25519 key at `<secrets>/ssh-ed25519`, keep the directory mode 700 and key mode 600, and temporarily authorize only its public key on the owned host. The test uses this identity rather than asking Keychain to grant a test runner access to the installed client's private key. Other pairing secrets and the test's stable host/device records remain under these ignored build directories. Reuse this state across repetitions; a new device ID combined with an already paired certificate is a different Sunshine identity.

The host must provide an isolated 1920×1080 X11 display and the input-counter fixture from `crates/kite-app/examples/desktop_fixture.py`, with its own Sunshine config/state on base port 48200. Keep the host's real display, services, credentials and pairing state independent. The QA Sunshine may use the host's existing management credentials, with a separate credentials/state file; do not print them. Disable its system tray for headless operation. Run only the owned processes and remove their authorization/resources afterward.

The test opens or resumes its own Codex session, verifies an actual tool-free response, presents actual H.264 frames through VideoToolbox/Metal, sends controlled input through `RemoteInput`, reconnects history and archives only sessions in its disposable project. Check the remote `input.json` separately for the expected event-count increments, F8 and exact Chinese-text match. Native sound stays muted; the offline Opus test verifies decoding independently.
